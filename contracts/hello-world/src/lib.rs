#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, panic_with_error, 
    Address, BytesN, Env, Symbol, Vec, token, Map
};

// ======================
// CONSTANTS & EVENT TYPES
// ======================
const JOB_CRT: Symbol = symbol_short!("JOB_CRT");      // Job created event
const JOB_FUND: Symbol = symbol_short!("JOB_FUND");    // Job funded event
const TAL_SEL: Symbol = symbol_short!("TAL_SEL");      // Talent selected event
const WRK_SUB: Symbol = symbol_short!("WRK_SUB");      // Work submitted event
const MIL_APR: Symbol = symbol_short!("MIL_APR");      // Milestone approved event
const DIS_RIS: Symbol = symbol_short!("DIS_RIS");      // Dispute raised event
const DIS_RES: Symbol = symbol_short!("DIS_RES");      // Dispute resolved event
const JOB_CANC: Symbol = symbol_short!("JOB_CANC");    // Job cancelled event
const RE_ENTRY: Symbol = symbol_short!("RE_ENTRY");    // Reentrancy guard
const TOKEN_ID: Symbol = symbol_short!("TOKEN_ID");    // Payment token ID
const ARB_REG: Symbol = symbol_short!("ARB_REG");      // Arbitrator registry
const ARB_FEE: i128 = 5;                              // Default arbitration fee (5%)

// ==============
// ERROR HANDLING
// ==============
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unauthorized = 1,       // Caller lacks permission
    InvalidState = 2,       // Invalid contract state
    InvalidInput = 3,       // Bad function parameters
    TalentExists = 4,       // Talent already selected
    MilestonePending = 5,   // Milestone not submitted
    NotSubmitted = 6,       // Work not submitted
    InvalidIndex = 7,       // Bad milestone index
    AmountRequired = 8,     // Payment amount <= 0
    Reentrancy = 9,         // Reentrant call detected
    JobNotFound = 10,       // Job doesn't exist
    InsufficientFunds = 11, // Not enough tokens
    TokenNotSet = 12,       // Payment token missing
    DeadlinePassed = 13,    // Time constraint failed
    ArbitrationPending = 14, // Dispute already exists
    NotArbitrator = 15,     // Caller not arbitrator
    JobCompleted = 16,      // Job already finished
    ClientOnly = 17,        // Client-restricted action
    TalentOnly = 18,        // Talent-restricted action
}

// ================
// STATE DEFINITIONS
// ================
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobState {
    Created,        // Job created but unfunded
    Funded,         // Funds deposited, no talent
    Active,         // Talent selected, work ongoing
    Completed,      // All milestones approved
    Disputed,       // Dispute raised
    Cancelled,      // Job cancelled by client
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MilestoneState {
    Pending,        // Not yet started
    Submitted,      // Work delivered
    Approved,       // Client accepted
    Rejected,       // Client rejected
    Paid,           // Payment released
    Disputed,       // Under arbitration
}

// =================
// DATA STRUCTURES
// =================
#[contracttype]
pub struct Milestone {
    description: BytesN<32>, // Milestone title/description
    amount: i128,            // Payment amount
    state: MilestoneState,   // Current status
    submission_data: BytesN<32>, // Work deliverables
    deadline: u64,           // Completion deadline (timestamp)
    submitted_at: Option<u64>, // Submission time
}

#[contracttype]
pub struct Job {
    client: Address,         // Job creator
    talent: Option<Address>, // Hired professional
    title: BytesN<32>,       // Job title
    total_value: i128,       // Total contract value
    amount_paid: i128,       // Total paid out
    state: JobState,         // Current status
    milestones: Vec<Milestone>, // Payment milestones
    escrow_balance: i128,    // Funds held in contract
    created_at: u64,         // Creation timestamp
    dispute_raised_by: Option<Address>, // Dispute initiator
    selected_arbitrator: Option<Address>, // Chosen arbitrator
    cancellation_fee: i128,  // Penalty for early cancel
}

#[contracttype]
pub struct Arbitrator {
    address: Address,        // Arbiter address
    fee_percentage: i128,    // Service fee (0-100)
    reputation: u32,         // Success score (0-100)
    cases_handled: u32,      // Total disputes resolved
    specialization: BytesN<32>, // Area of expertise
}

#[contract]
pub struct DecentralizedJobMarket;

#[contractimpl]
impl DecentralizedJobMarket {
    // ==============
    // INITIALIZATION
    // ==============
    /// Initialize contract with payment token
    /// @param env: Soroban environment
    /// @param token_id: Stellar asset contract ID
    pub fn initialize(env: Env, token_id: BytesN<32>) {
        if env.storage().has(&TOKEN_ID) {
            panic_with_error!(&env, Error::InvalidState);
        }
        env.storage().set(&TOKEN_ID, &token_id);
    }

    // ================
    // JOB LIFE CYCLE
    // ================
    /// Create new job with milestones
    /// @param env: Soroban environment
    /// @param client: Job creator address
    /// @param title: Job title (32 bytes max)
    /// @param descriptions: Milestone descriptions
    /// @param amounts: Milestone payments
    /// @param deadlines: Milestone deadlines (timestamps)
    /// @return job_id: Created job identifier
    pub fn create_job(
        env: Env,
        client: Address,
        title: BytesN<32>,
        descriptions: Vec<BytesN<32>>,
        amounts: Vec<i128>,
        deadlines: Vec<u64>,
    ) -> u32 {
        client.require_auth();
        Self::check_reentrancy(&env);

        // Validate inputs
        if descriptions.len() != amounts.len() || amounts.len() != deadlines.len() {
            panic_with_error!(&env, Error::InvalidInput);
        }

        let total_value: i128 = amounts.iter().sum();
        if total_value <= 0 {
            panic_with_error!(&env, Error::AmountRequired);
        }

        // Create milestones
        let mut milestones = Vec::new(&env);
        for i in 0..descriptions.len() {
            if *amounts.get(i).unwrap() <= 0 {
                panic_with_error!(&env, Error::AmountRequired);
            }

            milestones.push_back(
                &env,
                Milestone {
                    description: *descriptions.get(i).unwrap(),
                    amount: *amounts.get(i).unwrap(),
                    state: MilestoneState::Pending,
                    submission_data: BytesN::from_array(&env, &[0; 32]),
                    deadline: *deadlines.get(i).unwrap(),
                    submitted_at: None,
                },
            );
        }

        // Create job
        let job = Job {
            client: client.clone(),
            talent: None,
            title,
            total_value,
            amount_paid: 0,
            state: JobState::Created,
            milestones,
            escrow_balance: 0,
            created_at: env.ledger().timestamp(),
            dispute_raised_by: None,
            selected_arbitrator: None,
            cancellation_fee: total_value / 10, // 10% cancellation fee
        };

        let job_id = Self::save_job(&env, &job);
        env.events().publish(
            (JOB_CRT, client),
            (job_id, title, total_value)
        );
        job_id
    }

    /// Fund job escrow with payment tokens
    /// @param env: Soroban environment
    /// @param client: Job creator
    /// @param job_id: Job identifier
    pub fn fund_job(env: Env, client: Address, job_id: u32) {
        client.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.client != client {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if job.state != JobState::Created {
            panic_with_error!(&env, Error::InvalidState);
        }

        // Transfer tokens to escrow
        let token_id = Self::get_token_id(&env);
        token::Client::new(&env, &token_id).transfer(
            &client,
            &env.current_contract_address(),
            &job.total_value
        );

        job.escrow_balance = job.total_value;
        job.state = JobState::Funded;
        Self::update_job(&env, job_id, &job);

        env.events().publish(
            (JOB_FUND, client),
            (job_id, job.total_value)
        );
    }

    /// Select talent for funded job
    /// @param env: Soroban environment
    /// @param client: Job creator
    /// @param job_id: Job identifier
    /// @param talent: Freelancer address
    pub fn select_talent(env: Env, client: Address, job_id: u32, talent: Address) {
        client.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.client != client {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if job.state != JobState::Funded {
            panic_with_error!(&env, Error::InvalidState);
        }
        if job.talent.is_some() {
            panic_with_error!(&env, Error::TalentExists);
        }

        job.talent = Some(talent.clone());
        job.state = JobState::Active;
        Self::update_job(&env, job_id, &job);

        env.events().publish(
            (TAL_SEL, client),
            (job_id, talent)
        );
    }

    // ====================
    // MILESTONE OPERATIONS
    // ====================
    /// Submit work for milestone
    /// @param env: Soroban environment
    /// @param talent: Freelancer address
    /// @param job_id: Job identifier
    /// @param milestone_idx: Milestone index
    /// @param data: Work submission data
    pub fn submit_milestone(
        env: Env,
        talent: Address,
        job_id: u32,
        milestone_idx: u32,
        data: BytesN<32>,
    ) {
        talent.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.state != JobState::Active {
            panic_with_error!(&env, Error::InvalidState);
        }
        if job.talent != Some(talent.clone()) {
            panic_with_error!(&env, Error::Unauthorized);
        }

        let mut milestone = job.milestones.get(milestone_idx)
            .unwrap_or_else(|| panic_with_error!(&env, Error::InvalidIndex));
            
        if milestone.state != MilestoneState::Pending {
            panic_with_error!(&env, Error::MilestonePending);
        }

        // Check deadline
        if env.ledger().timestamp() > milestone.deadline {
            panic_with_error!(&env, Error::DeadlinePassed);
        }

        milestone.state = MilestoneState::Submitted;
        milestone.submission_data = data;
        milestone.submitted_at = Some(env.ledger().timestamp());
        job.milestones.set(milestone_idx, milestone);
        Self::update_job(&env, job_id, &job);

        env.events().publish(
            (WRK_SUB, talent),
            (job_id, milestone_idx, data)
        );
    }

    /// Approve milestone and release payment
    /// @param env: Soroban environment
    /// @param client: Job creator
    /// @param job_id: Job identifier
    /// @param milestone_idx: Milestone index
    pub fn approve_milestone(
        env: Env,
        client: Address,
        job_id: u32,
        milestone_idx: u32,
    ) {
        client.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.client != client {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if job.state != JobState::Active {
            panic_with_error!(&env, Error::InvalidState);
        }

        let mut milestone = job.milestones.get(milestone_idx)
            .unwrap_or_else(|| panic_with_error!(&env, Error::InvalidIndex));
            
        if milestone.state != MilestoneState::Submitted {
            panic_with_error!(&env, Error::NotSubmitted);
        }

        // Transfer payment
        let token_id = Self::get_token_id(&env);
        token::Client::new(&env, &token_id).transfer(
            &env.current_contract_address(),
            &job.talent.unwrap(),
            &milestone.amount
        );

        // Update state
        milestone.state = MilestoneState::Paid;
        job.milestones.set(milestone_idx, milestone);
        job.amount_paid += milestone.amount;
        job.escrow_balance -= milestone.amount;

        // Check completion
        if job.milestones.iter().all(|m| matches!(m.state, MilestoneState::Paid)) {
            job.state = JobState::Completed;
        }

        Self::update_job(&env, job_id, &job);
        env.events().publish(
            (MIL_APR, client),
            (job_id, milestone_idx, milestone.amount)
        );
    }

    // =================
    // DISPUTE RESOLUTION
    // =================
    /// Raise dispute for job/milestone
    /// @param env: Soroban environment
    /// @param caller: Dispute initiator
    /// @param job_id: Job identifier
    /// @param milestone_idx: Optional milestone index
    /// @param arbitrator: Chosen arbitrator address
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        job_id: u32,
        milestone_idx: Option<u32>,
        arbitrator: Address,
    ) {
        caller.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.state == JobState::Disputed {
            panic_with_error!(&env, Error::ArbitrationPending);
        }
        if job.state != JobState::Active {
            panic_with_error!(&env, Error::InvalidState);
        }

        // Verify caller is client or talent
        let is_client = job.client == caller;
        let is_talent = job.talent == Some(caller.clone());
        if !is_client && !is_talent {
            panic_with_error!(&env, Error::Unauthorized);
        }

        // Verify arbitrator exists
        if !Self::is_arbitrator(&env, &arbitrator) {
            panic_with_error!(&env, Error::NotArbitrator);
        }

        // If milestone specified, validate it
        if let Some(idx) = milestone_idx {
            let milestone = job.milestones.get(idx)
                .unwrap_or_else(|| panic_with_error!(&env, Error::InvalidIndex));
            if milestone.state != MilestoneState::Submitted {
                panic_with_error!(&env, Error::NotSubmitted);
            }
        }

        // Update job state
        job.state = JobState::Disputed;
        job.dispute_raised_by = Some(caller.clone());
        job.selected_arbitrator = Some(arbitrator.clone());
        Self::update_job(&env, job_id, &job);

        env.events().publish(
            (DIS_RIS, caller),
            (job_id, milestone_idx, arbitrator)
        );
    }

    /// Resolve dispute (arbitrator only)
    /// @param env: Soroban environment
    /// @param arbitrator: Arbitrator address
    /// @param job_id: Job identifier
    /// @param milestone_idx: Optional milestone index
    /// @param decision: true=approve, false=reject
    pub fn resolve_dispute(
        env: Env,
        arbitrator: Address,
        job_id: u32,
        milestone_idx: Option<u32>,
        decision: bool,
    ) {
        arbitrator.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.state != JobState::Disputed {
            panic_with_error!(&env, Error::InvalidState);
        }
        if job.selected_arbitrator != Some(arbitrator.clone()) {
            panic_with_error!(&env, Error::NotArbitrator);
        }

        // Calculate arbitrator fee
        let token_id = Self::get_token_id(&env);
        let fee_amount = job.total_value * ARB_FEE / 100;
        
        // Pay arbitrator
        token::Client::new(&env, &token_id).transfer(
            &env.current_contract_address(),
            &arbitrator,
            &fee_amount
        );

        // Process decision
        if decision {
            if let Some(idx) = milestone_idx {
                Self::approve_milestone_internal(&env, &mut job, idx);
            } else {
                Self::approve_all_milestones(&env, &mut job);
            }
        } else {
            if let Some(idx) = milestone_idx {
                Self::reject_milestone(&env, &mut job, idx);
            } else {
                Self::reject_all_milestones(&env, &mut job);
            }
        }

        // Update job state
        job.escrow_balance -= fee_amount;
        job.state = if job.milestones.iter().all(|m| matches!(m.state, MilestoneState::Paid)) {
            JobState::Completed
        } else {
            JobState::Active
        };
        job.dispute_raised_by = None;
        job.selected_arbitrator = None;
        Self::update_job(&env, job_id, &job);

        env.events().publish(
            (DIS_RES, arbitrator),
            (job_id, milestone_idx, decision, fee_amount)
        );
    }

    // ==============
    // JOB CANCELLATION
    // ==============
    /// Cancel job and refund remaining funds
    /// @param env: Soroban environment
    /// @param client: Job creator
    /// @param job_id: Job identifier
    pub fn cancel_job(env: Env, client: Address, job_id: u32) {
        client.require_auth();
        Self::check_reentrancy(&env);

        let mut job = Self::get_job(&env, job_id);
        if job.client != client {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if matches!(job.state, JobState::Completed | JobState::Cancelled) {
            panic_with_error!(&env, Error::JobCompleted);
        }

        let token_id = Self::get_token_id(&env);
        let refund_amount = job.escrow_balance - job.cancellation_fee;

        // Pay cancellation fee to talent if hired
        if let Some(talent) = &job.talent {
            token::Client::new(&env, &token_id).transfer(
                &env.current_contract_address(),
                talent,
                &job.cancellation_fee
            );
        } else {
            // If no talent, fee goes back to client
            refund_amount += job.cancellation_fee;
        }

        // Refund remaining to client
        if refund_amount > 0 {
            token::Client::new(&env, &token_id).transfer(
                &env.current_contract_address(),
                &client,
                &refund_amount
            );
        }

        job.state = JobState::Cancelled;
        job.escrow_balance = 0;
        Self::update_job(&env, job_id, &job);

        env.events().publish(
            (JOB_CANC, client),
            (job_id, refund_amount, job.cancellation_fee)
        );
    }

    // =================
    // ARBITRATOR MANAGEMENT
    // =================
    /// Register as arbitrator
    /// @param env: Soroban environment
    /// @param arbitrator: Arbitrator address
    /// @param specialization: Area of expertise
    pub fn register_arbitrator(
        env: Env,
        arbitrator: Address,
        specialization: BytesN<32>,
    ) {
        arbitrator.require_auth();
        Self::check_reentrancy(&env);

        let mut arbitrators = Self::get_arbitrators(&env);
        if arbitrators.contains_key(arbitrator.clone()) {
            panic_with_error!(&env, Error::InvalidState);
        }

        arbitrators.set(
            arbitrator.clone(),
            Arbitrator {
                address: arbitrator.clone(),
                fee_percentage: ARB_FEE,
                reputation: 80, // Initial reputation
                cases_handled: 0,
                specialization,
            },
        );

        env.storage().set(&ARB_REG, &arbitrators);
        env.events().publish(
            (ARB_REG, arbitrator),
            specialization
        );
    }

    // ====================
    // INTERNAL HELPERS
    // ====================
    fn approve_milestone_internal(env: &Env, job: &mut Job, idx: u32) {
        let mut milestone = job.milestones.get(idx)
            .unwrap_or_else(|| panic_with_error!(env, Error::InvalidIndex));
            
        let token_id = Self::get_token_id(env);
        token::Client::new(env, &token_id).transfer(
            &env.current_contract_address(),
            &job.talent.unwrap(),
            &milestone.amount
        );

        milestone.state = MilestoneState::Paid;
        job.milestones.set(idx, milestone);
        job.amount_paid += milestone.amount;
        job.escrow_balance -= milestone.amount;
    }

    fn approve_all_milestones(env: &Env, job: &mut Job) {
        for i in 0..job.milestones.len() {
            let mut milestone = job.milestones.get(i).unwrap();
            if matches!(milestone.state, MilestoneState::Submitted) {
                Self::approve_milestone_internal(env, job, i);
            }
        }
    }

    fn reject_milestone(env: &Env, job: &mut Job, idx: u32) {
        let mut milestone = job.milestones.get(idx)
            .unwrap_or_else(|| panic_with_error!(env, Error::InvalidIndex));
            
        milestone.state = MilestoneState::Rejected;
        milestone.submission_data = BytesN::from_array(env, &[0; 32]);
        job.milestones.set(idx, milestone);
    }

    fn reject_all_milestones(env: &Env, job: &mut Job) {
        for i in 0..job.milestones.len() {
            let mut milestone = job.milestones.get(i).unwrap();
            if matches!(milestone.state, MilestoneState::Submitted) {
                milestone.state = MilestoneState::Rejected;
                milestone.submission_data = BytesN::from_array(env, &[0; 32]);
                job.milestones.set(i, milestone);
            }
        }
    }

    fn check_reentrancy(env: &Env) {
        if env.storage().has(&RE_ENTRY) {
            panic_with_error!(env, Error::Reentrancy);
        }
        env.storage().set(&RE_ENTRY, &true);
    }

    fn save_job(env: &Env, job: &Job) -> u32 {
        let mut count = env.storage().get(&symbol_short!("JOB_CNT"))
            .unwrap_or(Ok(0u32))
            .unwrap();
        count += 1;
        env.storage().set(&symbol_short!("JOB_CNT"), &count);
        env.storage().set(&Self::job_key(count), job);
        count
    }

    fn update_job(env: &Env, job_id: u32, job: &Job) {
        env.storage().set(&Self::job_key(job_id), job);
    }

    fn get_job(env: &Env, job_id: u32) -> Job {
        env.storage()
            .get(&Self::job_key(job_id))
            .unwrap_or_else(|| panic_with_error!(env, Error::JobNotFound))
            .unwrap()
    }

    fn get_token_id(env: &Env) -> BytesN<32> {
        env.storage().get(&TOKEN_ID))
            .unwrap_or_else(|| panic_with_error!(env, Error::TokenNotSet))
            .unwrap()
    }

    fn get_arbitrators(env: &Env) -> Map<Address, Arbitrator> {
        env.storage().get(&ARB_REG))
            .unwrap_or_else(|| Ok(Map::new(env)))
            .unwrap()
    }

    fn is_arbitrator(env: &Env, address: &Address) -> bool {
        Self::get_arbitrators(env).contains_key(address.clone())
    }

    fn job_key(job_id: u32) -> BytesN<32> {
        BytesN::from_array(&Env::default(), &{
            let mut arr = [0u8; 32];
            arr[..4].copy_from_slice(&job_id.to_be_bytes());
            arr
        })
    }
}