#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract]
mod staking {
    use ink_storage::{
        traits::{PackedLayout, SpreadAllocate, SpreadLayout},
        Mapping,
    };

    // ===== Events

    #[ink(event)]
    pub struct Staked {
        user: AccountId,
        amount: Balance,
    }

    #[ink(event)]
    pub struct Unstaked {
        user: AccountId,
        amount: Balance,
    }

    #[ink(event)]
    pub struct Claimed {
        user: AccountId,
        amount: Balance,
    }

    // ===== Errors

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum StakingError {
        UnstakeError(String),
        ClaimingRewardError(String),
        Other(String),
    }

    // ===== Custom structs

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode, SpreadLayout, PackedLayout)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct StakingPosition {
        pub stake_amount: Balance,
        pub last_action_block: BlockNumber,
    }

    // ===== Contract storage

    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Staking {
        apy: u64,
        stake_positions: Mapping<AccountId, StakingPosition>,
        staked_addresses: Vec<AccountId>,
    }

    impl Staking {
        #[ink(constructor)]
        pub fn new(apy: u64) -> Self {
            ink_lang::utils::initialize_contract(|contract: &mut Self| {
                contract.apy = apy;
            })
        }

        #[ink(message, payable)]
        pub fn stake(&mut self) -> Result<(), StakingError> {
            let transferred_amount = self.env().transferred_value();
            assert!(transferred_amount > 0, "Must stake more than 0");

            let caller = self.env().caller();
            if let Some(staking_position) = self.stake_positions.get(caller) {
                let balance = staking_position.stake_amount;

                if let Some(new_balance) = balance.checked_add(transferred_amount) {
                    let new_staking_position = StakingPosition {
                        stake_amount: new_balance,
                        last_action_block: staking_position.last_action_block,
                    };
                    self.stake_positions.insert(caller, &new_staking_position);
                } else {
                    return Err(StakingError::Other(
                        "Failed while adding balances".to_owned(),
                    ));
                }
            } else {
                self.stake_positions.insert(
                    caller,
                    &StakingPosition {
                        stake_amount: transferred_amount,
                        last_action_block: self.env().block_number(),
                    },
                );
            }

            self.staked_addresses.push(caller);
            self.env().emit_event(Staked {
                user: self.env().caller(),
                amount: transferred_amount,
            });

            Ok(())
        }

        #[ink(message)]
        pub fn unstake(&mut self, unstake_amount: Balance) -> Result<(), StakingError> {
            assert!(unstake_amount > 0, "Must unstake more than 0");

            let caller = self.env().caller();
            let staking_position = self.stake_positions.get(&caller);
            if let Some(user_stake) = staking_position {
                if unstake_amount > user_stake.stake_amount {
                    return Err(StakingError::UnstakeError(
                        "unstake amount cannot be greater than staked amount".to_owned(),
                    ));
                } else {
                    if let Some(rest_stake) = user_stake.stake_amount.checked_sub(unstake_amount) {
                        if rest_stake == 0 {
                            let idx = self
                                .staked_addresses
                                .iter()
                                .position(|x| *x == caller)
                                .unwrap();
                            self.staked_addresses.remove(idx);

                            if let Err(e) = self.claim_reward() {
                                return Err(StakingError::Other(format!(
                                    "Failed to claim all the rewards after unstaking: {:?}",
                                    e
                                )));
                            }
                        }

                        
                        if self.env().transfer(caller, unstake_amount).is_err() {
                            panic!("failed to transfer unstaked amount")
                        }

                        self.stake_positions.insert(
                            caller,
                            &StakingPosition {
                                stake_amount: rest_stake,
                                last_action_block: self.env().block_number(),
                            },
                        );

                        self.env().emit_event(Unstaked {
                            user: caller,
                            amount: unstake_amount,
                        });
                    } else {
                        return Err(StakingError::Other(
                            "Overflow error while substractiong stakes".to_owned(),
                        ));
                    }
                }
            } else {
                return Err(StakingError::UnstakeError(
                    "can only unstake if user has already staked".to_owned(),
                ));
            }

            Ok(())
        }

        #[ink(message)]
        pub fn claim_reward(&mut self) -> Result<(), StakingError> {
            let caller = self.env().caller();
            let reward = self.rewards_for_user(caller);

            if let Some(staking_position) = self.stake_positions.get(caller) {
                self.stake_positions.insert(
                    caller,
                    &StakingPosition {
                        stake_amount: staking_position.stake_amount,
                        last_action_block: self.env().block_number(),
                    },
                );

                if reward > 0 {
                    if self.env().transfer(caller, reward).is_err() {
                        return Err(StakingError::ClaimingRewardError(
                            "failed to transfer claimed reward to user".to_owned(),
                        ));
                    }

                    self.env().emit_event(Claimed {
                        amount: reward,
                        user: caller,
                    });
                }
            } else {
                return Err(StakingError::ClaimingRewardError(
                    "user doesnt seem to have a stake".to_owned(),
                ));
            }

            Ok(())
        }

        #[ink(message)]
        pub fn get_account_stake(&self, account: AccountId) -> Balance {
            match self.stake_positions.get(account) {
                Some(position) => position.stake_amount,
                _ => Balance::from(0u128),
            }
        }

        #[ink(message)]
        pub fn rewards_for_user(&self, user: AccountId) -> Balance {
            let staking_position = self.stake_positions.get(user);
            match staking_position {
                Some(stake) => self.calculate_rewards(&stake),
                _ => Balance::from(0u128),
            }
        }

        fn calculate_rewards(&self, staking_position: &StakingPosition) -> Balance {
            let current_block = self.env().block_number();
            if current_block <= staking_position.last_action_block {
                return Balance::from(0u128);
            }

            current_block
                .checked_sub(staking_position.last_action_block)
                .unwrap()
                .into()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use ink_lang as ink;
        use ink_lang::codegen::Env;

        use ink_env::{
            test::{default_accounts, get_account_balance, EmittedEvent},
            AccountId,
        };

        type Event = <Staking as ink::reflect::ContractEventBase>::Type;

        fn assert_staked_event(
            event: &EmittedEvent,
            expected_user: &AccountId,
            expected_amount: Balance,
        ) {
            let decoded_event = <Event as scale::Decode>::decode(&mut &event.data[..])
                .expect("encountered invalid contract event data buffer");
            if let Event::Staked(Staked { user, amount }) = decoded_event {
                assert_eq!(user, *expected_user);
                assert_eq!(amount, expected_amount);
            } else {
                panic!("encountered unexpected event kind: expected a Staked event")
            }
        }

        fn assert_unstaked_event(
            event: &EmittedEvent,
            expected_user: &AccountId,
            expected_amount: Balance,
        ) {
            let decoded_event = <Event as scale::Decode>::decode(&mut &event.data[..])
                .expect("encountered invalid contract event data buffer");
            if let Event::Unstaked(Unstaked { user, amount }) = decoded_event {
                assert_eq!(user, *expected_user);
                assert_eq!(amount, expected_amount);
            } else {
                panic!("encountered unexpected event kind: expected a Unstaked event")
            }
        }

        fn assert_claimed_event(
            event: &EmittedEvent,
            expected_user: &AccountId,
            expected_amount: Balance,
        ) {
            let decoded_event = <Event as scale::Decode>::decode(&mut &event.data[..])
                .expect("encountered invalid contract event data buffer");
            if let Event::Claimed(Claimed { user, amount }) = decoded_event {
                assert_eq!(user, *expected_user);
                assert_eq!(amount, expected_amount);
            } else {
                panic!("encountered unexpected event kind: expected a Claimed event")
            }
        }

        #[ink::test]
        fn deployment_works() {
            let staking = Staking::new(1000);
            assert_eq!(staking.apy, 1000);
            assert_eq!(staking.staked_addresses, Vec::default());
        }

        #[ink::test]
        fn first_time_staking_should_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);

            let stake = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(stake, Ok(()));
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            // contract now has 10 coins more
            let contract_balance = get_account_balance::<ink_env::DefaultEnvironment>(
                staking_contract_instance.env().account_id(),
            )
            .unwrap();
            assert_eq!(1000010, contract_balance);

            assert!(staking_contract_instance.staked_addresses.contains(&alice));

            let emitted_events = ink_env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(1, emitted_events.len());
            assert_staked_event(&emitted_events[0], &alice, 10);
        }

        #[ink::test]
        fn increasing_existing_stake_should_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);

            let stake = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(stake, Ok(()));
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            let stake_again = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(stake_again, Ok(()));
            assert_eq!(staking_contract_instance.get_account_stake(alice), 20);
        }

        #[ink::test]
        #[should_panic(expected = "Must stake more than 0")]
        fn staking_zero_should_not_be_allowed() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 0);
        }

        #[ink::test]
        fn claiming_should_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);
            let alice_balance = get_account_balance::<ink_env::DefaultEnvironment>(alice).unwrap();
            assert_eq!(alice_balance, 1000000);

            let mut staking_contract_instance = Staking::new(1000);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);

            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            for _ in 0..5 {
                ink_env::test::advance_block::<ink_env::DefaultEnvironment>();
            }

            let to_be_claimed = staking_contract_instance.rewards_for_user(alice);
            assert_eq!(5, to_be_claimed);

            let claim = staking_contract_instance.claim_reward();
            assert_eq!(claim, Ok(()));

            let alice_balance = get_account_balance::<ink_env::DefaultEnvironment>(alice).unwrap();
            assert_eq!(alice_balance, 1000015);

            let emitted_events = ink_env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(2, emitted_events.len());
            assert_staked_event(&emitted_events[0], &alice, 10);
            assert_claimed_event(&emitted_events[1], &alice, 5);
        }

        #[ink::test]
        fn claiming_while_not_staked_should_not_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);

            let claim = staking_contract_instance.claim_reward();
            assert_eq!(
                claim,
                Err(StakingError::ClaimingRewardError(
                    "user doesnt seem to have a stake".to_owned()
                ))
            )
        }

        #[ink::test]
        fn unstake_should_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            let unstake_result = staking_contract_instance.unstake(10);
            assert_eq!(unstake_result, Ok(()));
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);
            assert_eq!(staking_contract_instance.staked_addresses.contains(&alice), false);

            let emitted_events = ink_env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(2, emitted_events.len());
            assert_staked_event(&emitted_events[0], &alice, 10);
            assert_unstaked_event(&emitted_events[1], &alice, 10);
        }

        #[ink::test]
        fn partial_unstake_should_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            let unstake_result = staking_contract_instance.unstake(5);
            assert_eq!(unstake_result, Ok(()));
            assert_eq!(staking_contract_instance.get_account_stake(alice), 5);
            assert!(staking_contract_instance.staked_addresses.contains(&alice));

            let emitted_events = ink_env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(2, emitted_events.len());
            assert_staked_event(&emitted_events[0], &alice, 10);
            assert_unstaked_event(&emitted_events[1], &alice, 5);
        }

        #[ink::test]
        #[should_panic(expected = "Must unstake more than 0")]
        fn unstake_zero_not_allowed() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            let _ = staking_contract_instance.unstake(0);
        }

        #[ink::test]
        fn unstake_more_than_staked_should_not_work() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 0);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            let unstake = staking_contract_instance.unstake(11);
            assert_eq!(
                unstake,
                Err(StakingError::UnstakeError(
                    "unstake amount cannot be greater than staked amount".to_owned()
                ))
            )
        }

        #[ink::test]
        fn unstake_should_work_iff_user_has_staked() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);
            let unstake = staking_contract_instance.unstake(1);
            assert_eq!(
                unstake,
                Err(StakingError::UnstakeError(
                    "can only unstake if user has already staked".to_owned()
                ))
            )
        }

        #[ink::test]
        fn unstake_all_must_trigger_reward_claiming() {
            let alice = default_accounts::<ink_env::DefaultEnvironment>().alice;
            ink_env::test::set_caller::<ink_env::DefaultEnvironment>(alice);

            let mut staking_contract_instance = Staking::new(1000);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            for _ in 0..5 {
                ink_env::test::advance_block::<ink_env::DefaultEnvironment>();
            }

            let to_be_claimed = staking_contract_instance.rewards_for_user(alice);
            assert_eq!(5, to_be_claimed);

            let unstake_result = staking_contract_instance.unstake(10);
            assert_eq!(Ok(()), unstake_result);

            let to_be_claimed = staking_contract_instance.rewards_for_user(alice);
            assert_eq!(0, to_be_claimed);

            let alice_balance = get_account_balance::<ink_env::DefaultEnvironment>(alice).unwrap();
            assert_eq!(1000025, alice_balance);
        }
    }
}
