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
                        last_action_block: self.env().block_number(),
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

        pub fn unstake(&mut self, unstake_amount: Balance) -> Result<(), StakingError> {
            Ok(())
        }

        pub fn claim_reward(&mut self) -> Result<(), StakingError> {
            Ok(())
        }

        #[ink(message)]
        pub fn get_account_stake(&self, account: AccountId) -> Balance {
            match self.stake_positions.get(account) {
                Some(position) => position.stake_amount,
                _ => Balance::from(0u128),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use ink_lang as ink;

        use ink_env::{
            test::{
                default_accounts, get_account_balance, recorded_events, DefaultAccounts,
                EmittedEvent,
            },
            AccountId,
        };

        type Event = <Staking as ink::reflect::ContractEventBase>::Type;

        fn assert_staked_event(
            event: &ink_env::test::EmittedEvent,
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
            event: &ink_env::test::EmittedEvent,
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
            event: &ink_env::test::EmittedEvent,
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
            let alice_balance =
                ink_env::test::get_account_balance::<ink_env::DefaultEnvironment>(alice).unwrap();
            assert_eq!(alice_balance, 1000000);

            let mut staking_contract_instance = Staking::new(1000);

            let _ = ink_env::pay_with_call!(staking_contract_instance.stake(), 10);
            assert_eq!(staking_contract_instance.get_account_stake(alice), 10);

            for _ in 0..5 {
                ink_env::test::advance_block::<ink_env::DefaultEnvironment>();
            }

            let claim = staking_contract_instance.claim_reward();
            assert_eq!(claim, Ok(()));

            let alice_balance =
                ink_env::test::get_account_balance::<ink_env::DefaultEnvironment>(alice).unwrap();
            assert_eq!(alice_balance, 1000005);

            let emitted_events = ink_env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(1, emitted_events.len());
            assert_claimed_event(&emitted_events[0], &alice, 5);
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
                    "cannot claim rewards with no stake".to_owned()
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

            let emitted_events = ink_env::test::recorded_events().collect::<Vec<_>>();
            assert_eq!(1, emitted_events.len());
            assert_unstaked_event(&emitted_events[0], &alice, 10);
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
                    "cannot stake more than staked amount".to_owned()
                ))
            )
        }
    }
}
