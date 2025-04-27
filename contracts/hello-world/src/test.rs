// #![cfg(test)]

// use super::*;
// use soroban_sdk::{vec, Env, String};

// #[test]
// fn test() {
//     let env = Env::default();
//     let contract_id = env.register(Contract, ());
//     let client = ContractClient::new(&env, &contract_id);

//     let words = client.hello(&String::from_str(&env, "Dev"));
//     assert_eq!(
//         words,
//         vec![
//             &env,
//             String::from_str(&env, "Hello"),
//             String::from_str(&env, "Dev"),
//         ]
//     );
// }
#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events}, 
    vec, 
    Address, 
    BytesN, 
    Env, 
    IntoVal, 
    Symbol
};

use crate::{
    JobMarketplaceContract, 
    JobMarketplaceContractClient, 
    Error, 
    JobStatus, 
    MilestoneStatus
};

#[test]
fn test_create_job() {
    let env = Env::default();
    let contract_id = env.register_contract(None, JobMarketplaceContract);
    let client = JobMarketplaceContractClient::new(&env, &contract_id);

    let client_address = Address::random(&env);
    env.mock_all_auths(); // This will mock all auth requirements
    
    let title = BytesN::from_array(&env, &[1; 32]);
    let descriptions = vec![
        &env,
        BytesN::from_array(&env, &[2; 32]),
        BytesN::from_array(&env, &[3; 32]),
    ];
    let amounts = vec![&env, 100, 200];

    // Test successful job creation
    let job_id = client.create_job(
        &client_address,
        &title,
        &descriptions,
        &amounts,
    );
    assert_eq!(job_id, 1);

    // Verify job count increased
    let job_count = env.storage().get(&Symbol::short("JOB_COUNT")).unwrap().unwrap();
    assert_eq!(job_count, 1u32);

    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    assert!(events.contains((
        contract_id.clone(),
        (Symbol::short("JOB_CREATED"), client_address.clone()).into_val(&env),
        (job_id, title, 300).into_val(&env)
    )));

    // Test invalid input (length mismatch)
    let bad_amounts = vec![&env, 100];
    let result = client.try_create_job(
        &client_address,
        &title,
        &descriptions,
        &bad_amounts,
    );
    assert_eq!(result, Err(Ok(Error::InvalidInput.into())));

    // Test invalid input (zero amount)
    let zero_amounts = vec![&env, 0, 0];
    let result = client.try_create_job(
        &client_address,
        &title,
        &descriptions,
        &zero_amounts,
    );
    assert_eq!(result, Err(Ok(Error::AmountMustBePositive.into())));
}

#[test]
fn test_select_talent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, JobMarketplaceContract);
    let client = JobMarketplaceContractClient::new(&env, &contract_id);

    let client_address = Address::random(&env);
    let talent_address = Address::random(&env);
    env.mock_all_auths(); // Mock all auth requirements
    
    let title = BytesN::from_array(&env, &[1; 32]);
    let descriptions = vec![&env, BytesN::from_array(&env, &[2; 32])];
    let amounts = vec![&env, 100];

    // Create job first
    let job_id = client.create_job(
        &client_address,
        &title,
        &descriptions,
        &amounts,
    );

    // Test successful talent selection
    client.select_talent(&client_address, &job_id, &talent_address);

    // Verify job state
    let job = client.get_job(&job_id);
    assert_eq!(job.talent, Some(talent_address.clone()));
    assert_eq!(job.status, JobStatus::InProgress);

    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 2);
    assert!(events.contains((
        contract_id.clone(),
        (Symbol::short("TALENT_SEL"), client_address.clone()).into_val(&env),
        (job_id, talent_address).into_val(&env)
    )));

    // Test unauthorized access
    let other_address = Address::random(&env);
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_select_talent(&other_address, &job_id, &talent_address);
    assert_eq!(result, Err(Ok(Error::Unauthorized.into())));

    // Test invalid state (already has talent)
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_select_talent(&client_address, &job_id, &talent_address);
    assert_eq!(result, Err(Ok(Error::TalentAlreadySelected.into())));
}

#[test]
fn test_submit_milestone() {
    let env = Env::default();
    let contract_id = env.register_contract(None, JobMarketplaceContract);
    let client = JobMarketplaceContractClient::new(&env, &contract_id);

    let client_address = Address::random(&env);
    let talent_address = Address::random(&env);
    env.mock_all_auths(); // Mock all auth requirements
    
    let title = BytesN::from_array(&env, &[1; 32]);
    let descriptions = vec![&env, BytesN::from_array(&env, &[2; 32])];
    let amounts = vec![&env, 100];

    // Create job and select talent
    let job_id = client.create_job(
        &client_address,
        &title,
        &descriptions,
        &amounts,
    );
    client.select_talent(&client_address, &job_id, &talent_address);

    // Test successful milestone submission
    let submission_data = BytesN::from_array(&env, &[3; 32]);
    client.submit_milestone(&talent_address, &job_id, &0, &submission_data);

    // Verify milestone state
    let job = client.get_job(&job_id);
    let milestone = job.milestones.get(0).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Submitted);
    assert_eq!(milestone.submission_data, submission_data);

    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 3);
    assert!(events.contains((
        contract_id.clone(),
        (Symbol::short("WORK_SUB"), talent_address.clone()).into_val(&env),
        (job_id, 0u32, submission_data).into_val(&env)
    )));

    // Test unauthorized access
    let other_address = Address::random(&env);
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_submit_milestone(&other_address, &job_id, &0, &submission_data);
    assert_eq!(result, Err(Ok(Error::Unauthorized.into())));

    // Test invalid milestone index
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_submit_milestone(&talent_address, &job_id, &1, &submission_data);
    assert_eq!(result, Err(Ok(Error::InvalidMilestoneIndex.into())));

    // Test milestone not pending
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_submit_milestone(&talent_address, &job_id, &0, &submission_data);
    assert_eq!(result, Err(Ok(Error::MilestoneNotPending.into())));
}

#[test]
fn test_approve_milestone() {
    let env = Env::default();
    let contract_id = env.register_contract(None, JobMarketplaceContract);
    let client = JobMarketplaceContractClient::new(&env, &contract_id);

    let client_address = Address::random(&env);
    let talent_address = Address::random(&env);
    env.mock_all_auths(); // Mock all auth requirements
    
    let title = BytesN::from_array(&env, &[1; 32]);
    let descriptions = vec![&env, BytesN::from_array(&env, &[2; 32])];
    let amounts = vec![&env, 100];

    // Create job, select talent, and submit milestone
    let job_id = client.create_job(
        &client_address,
        &title,
        &descriptions,
        &amounts,
    );
    client.select_talent(&client_address, &job_id, &talent_address);
    let submission_data = BytesN::from_array(&env, &[3; 32]);
    client.submit_milestone(&talent_address, &job_id, &0, &submission_data);

    // Test successful milestone approval
    client.approve_milestone(&client_address, &job_id, &0);

    // Verify milestone and job state
    let job = client.get_job(&job_id);
    let milestone = job.milestones.get(0).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Approved);
    assert_eq!(job.amount_paid, 100);
    assert_eq!(job.status, JobStatus::Completed);

    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 4);
    assert!(events.contains((
        contract_id.clone(),
        (Symbol::short("MILEST_APPR"), client_address.clone()).into_val(&env),
        (job_id, 0u32, 100, talent_address).into_val(&env)
    )));

    // Test unauthorized access
    let other_address = Address::random(&env);
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_approve_milestone(&other_address, &job_id, &0);
    assert_eq!(result, Err(Ok(Error::Unauthorized.into())));

    // Test invalid milestone state
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_approve_milestone(&client_address, &job_id, &0);
    assert_eq!(result, Err(Ok(Error::MilestoneNotSubmitted.into())));
}

#[test]
fn test_dispute_workflow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, JobMarketplaceContract);
    let client = JobMarketplaceContractClient::new(&env, &contract_id);

    let client_address = Address::random(&env);
    let talent_address = Address::random(&env);
    let arbitrator_address = Address::random(&env);
    env.mock_all_auths(); // Mock all auth requirements
    
    let title = BytesN::from_array(&env, &[1; 32]);
    let descriptions = vec![&env, BytesN::from_array(&env, &[2; 32])];
    let amounts = vec![&env, 100];

    // Create job, select talent, and submit milestone
    let job_id = client.create_job(
        &client_address,
        &title,
        &descriptions,
        &amounts,
    );
    client.select_talent(&client_address, &job_id, &talent_address);
    let submission_data = BytesN::from_array(&env, &[3; 32]);
    client.submit_milestone(&talent_address, &job_id, &0, &submission_data);

    // Test raising a dispute (by client)
    client.raise_dispute(&client_address, &job_id, &Some(0));

    // Verify job state
    let job = client.get_job(&job_id);
    assert_eq!(job.status, JobStatus::Disputed);
    assert_eq!(job.dispute_raised, Some(client_address.clone()));

    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 4);
    assert!(events.contains((
        contract_id.clone(),
        (Symbol::short("DISPUTE_RAISED"), client_address.clone()).into_val(&env),
        (job_id, Some(0u32)).into_val(&env)
    )));

    // Test resolving dispute (approve)
    env.mock_all_auths(); // Reset auth mocks
    client.resolve_dispute(&arbitrator_address, &job_id, &Some(0), &true);

    // Verify job state
    let job = client.get_job(&job_id);
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.amount_paid, 100);
    assert_eq!(job.milestones.get(0).unwrap().status, MilestoneStatus::Approved);

    // Verify event was emitted
    let events = env.events().all();
    assert_eq!(events.len(), 5);
    assert!(events.contains((
        contract_id.clone(),
        (Symbol::short("DISPUTE_RES"), arbitrator_address.clone()).into_val(&env),
        (job_id, Some(0u32), true).into_val(&env)
    )));

    // Test raising dispute on non-existent job
    env.mock_all_auths(); // Reset auth mocks
    let result = client.try_raise_dispute(&client_address, &999, &Some(0));
    assert_eq!(result, Err(Ok(Error::JobNotFound.into())));
}

#[test]
fn test_reentrancy_guard() {
    let env = Env::default();
    let contract_id = env.register_contract(None, JobMarketplaceContract);
    let client = JobMarketplaceContractClient::new(&env, &contract_id);

    let client_address = Address::random(&env);
    env.mock_all_auths(); // Mock all auth requirements
    
    let title = BytesN::from_array(&env, &[1; 32]);
    let descriptions = vec![&env, BytesN::from_array(&env, &[2; 32])];
    let amounts = vec![&env, 100];

    // This should succeed
    let job_id = client.create_job(
        &client_address,
        &title,
        &descriptions,
        &amounts,
    );

    // Verify guard was cleared
    assert!(!env.storage().has(&Symbol::short("REENTRANCY")));
}