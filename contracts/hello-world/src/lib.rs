// #![no_std]
// use soroban_sdk::{contract, contractimpl, vec, Env, String, Vec};

// #[contract]
// pub struct Contract;

// // This is a sample contract. Replace this placeholder with your own contract logic.
// // A corresponding test example is available in `test.rs`.
// //
// // For comprehensive examples, visit <https://github.com/stellar/soroban-examples>.
// // The repository includes use cases for the Stellar ecosystem, such as data storage on
// // the blockchain, token swaps, liquidity pools, and more.
// //
// // Refer to the official documentation:
// // <https://developers.stellar.org/docs/build/smart-contracts/overview>.
// #[contractimpl]
// impl Contract {
//     pub fn hello(env: Env, to: String) -> Vec<String> {
//         vec![&env, String::from_str(&env, "Hello"), to]
//     }
// }

// mod test;

#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, panic_with_error, Address, BytesN, Env, Symbol, Vec};

// Event symbols (all <= 9 chars)
const JOB_CRT: Symbol = symbol_short!("JOB_CRT");      // Job created
const MIL_ADD: Symbol = symbol_short!("MIL_ADD");     // Milestone added
const TAL_SEL: Symbol = symbol_short!("TAL_SEL");     // Talent selected
const WRK_SUB: Symbol = symbol_short!("WRK_SUB");     // Work submitted
const MIL_APR: Symbol = symbol_short!("MIL_APR");     // Milestone approved
const DIS_RIS: Symbol = symbol_short!("DIS_RIS");     // Dispute raised
const DIS_RES: Symbol = symbol_short!("DIS_RES");     // Dispute resolved

// Reentrancy guard
const RE_ENTRY: Symbol = symbol_short!("RE_ENTRY");   // Reentrancy guard

// Error types
#[contracttype]
pub enum Err {
    UnAuth = 1,         // Unauthorized
    BadState = 2,       // Invalid state
    BadIn = 3,          // Invalid input
    TalSet = 4,         // Talent exists
    MilPen = 5,         // Milestone pending
    NotSub = 6,         // Not submitted
    BadIdx = 7,         // Invalid index
    AmtPos = 8,         // Amount > 0 required
    ReEnt = 9,          // Reentrant call
    NoJob = 10,         // Job not found
}

// Job states
#[contracttype]
pub enum JobSt {
    Crt,        // Created
    Act,        // Active
    Done,       // Done
    Disp        // Disputed
}

// Milestone states
#[contracttype]
pub enum MileSt {
    Pen,        // Pending
    Sub,        // Submitted
    Apr,        // Approved
    Disp        // Disputed
}

// Milestone struct
#[contracttype]
pub struct Mile {
    desc: BytesN<32>,   // Description
    amt: i128,          // Amount
    st: MileSt,         // State
    data: BytesN<32>,   // Submission data
}

// Job struct
#[contracttype]
pub struct Job {
    cli: Address,       // Client
    tal: Option<Address>, // Talent
    tit: BytesN<32>,    // Title
    tot: i128,          // Total
    paid: i128,         // Paid
    st: JobSt,          // State
    miles: Vec<Mile>,   // Milestones
    disp_by: Option<Address>, // Disputer
}

#[contract]
pub struct JobMarket;

#[contractimpl]
impl JobMarket {
    pub fn create(
        env: Env,
        cli: Address,
        tit: BytesN<32>,
        descs: Vec<BytesN<32>>,
        amts: Vec<i128>,
    ) -> u32 {
        cli.require_auth();
        Self::chk_reent(&env);

        if descs.len() != amts.len() {
            panic_with_error!(&env, Err::BadIn);
        }

        let tot: i128 = amts.iter().sum();
        if tot <= 0 {
            panic_with_error!(&env, Err::AmtPos);
        }

        let mut miles = Vec::new(&env);
        for (i, desc) in descs.iter().enumerate() {
            let amt = amts.get(i).unwrap_or_else(|| {
                panic_with_error!(&env, Err::BadIn)
            });
            
            if *amt <= 0 {
                panic_with_error!(&env, Err::AmtPos);
            }
            
            miles.push_back(
                &env,
                Mile {
                    desc: *desc,
                    amt: *amt,
                    st: MileSt::Pen,
                    data: BytesN::from_array(&env, &[0; 32]),
                },
            );
        }

        let job = Job {
            cli: cli.clone(),
            tal: None,
            tit,
            tot,
            paid: 0,
            st: JobSt::Crt,
            miles,
            disp_by: None,
        };

        let id = Self::save_job(&env, &job);
        env.events().publish(
            (JOB_CRT, cli),
            (id, tit, tot),
        );

        id
    }

    pub fn sel_tal(
        env: Env,
        cli: Address,
        job_id: u32,
        tal: Address,
    ) {
        cli.require_auth();
        Self::chk_reent(&env);
        
        let mut job = Self::get_job(&env, job_id);

        if job.cli != cli {
            panic_with_error!(&env, Err::UnAuth);
        }
        if job.st != JobSt::Crt {
            panic_with_error!(&env, Err::BadState);
        }
        if job.tal.is_some() {
            panic_with_error!(&env, Err::TalSet);
        }

        job.tal = Some(tal.clone());
        job.st = JobSt::Act;
        Self::upd_job(&env, job_id, &job);
        
        env.events().publish(
            (TAL_SEL, cli),
            (job_id, tal),
        );

        env.storage().remove(&RE_ENTRY);
    }

    pub fn sub_mile(
        env: Env,
        tal: Address,
        job_id: u32,
        mile_idx: u32,
        data: BytesN<32>,
    ) {
        tal.require_auth();
        Self::chk_reent(&env);
        
        let mut job = Self::get_job(&env, job_id);

        if job.st != JobSt::Act {
            panic_with_error!(&env, Err::BadState);
        }
        if job.tal != Some(tal.clone()) {
            panic_with_error!(&env, Err::UnAuth);
        }

        let mut mile = job.miles.get(mile_idx)
            .unwrap_or_else(|| panic_with_error!(&env, Err::BadIdx));
            
        if mile.st != MileSt::Pen {
            panic_with_error!(&env, Err::MilPen);
        }

        mile.st = MileSt::Sub;
        mile.data = data;
        job.miles.set(mile_idx, mile);
        Self::upd_job(&env, job_id, &job);
        
        env.events().publish(
            (WRK_SUB, tal),
            (job_id, mile_idx, data),
        );

        env.storage().remove(&RE_ENTRY);
    }

    pub fn apr_mile(
        env: Env,
        cli: Address,
        job_id: u32,
        mile_idx: u32,
    ) {
        cli.require_auth();
        Self::chk_reent(&env);
        
        let mut job = Self::get_job(&env, job_id);

        if job.cli != cli {
            panic_with_error!(&env, Err::UnAuth);
        }
        if job.st != JobSt::Act {
            panic_with_error!(&env, Err::BadState);
        }

        let mut mile = job.miles.get(mile_idx)
            .unwrap_or_else(|| panic_with_error!(&env, Err::BadIdx));
            
        if mile.st != MileSt::Sub {
            panic_with_error!(&env, Err::NotSub);
        }

        mile.st = MileSt::Apr;
        job.miles.set(mile_idx, mile);
        job.paid += mile.amt;

        if job.miles.iter().all(|m| matches!(m.st, MileSt::Apr)) {
            job.st = JobSt::Done;
        }

        Self::upd_job(&env, job_id, &job);
        env.events().publish(
            (MIL_APR, cli),
            (job_id, mile_idx, mile.amt, job.tal.unwrap()),
        );

        env.storage().remove(&RE_ENTRY);
    }

    pub fn raise_disp(
        env: Env,
        caller: Address,
        job_id: u32,
        mile_idx: Option<u32>,
    ) {
        caller.require_auth();
        Self::chk_reent(&env);
        
        let mut job = Self::get_job(&env, job_id);

        let is_cli = job.cli == caller;
        let is_tal = job.tal == Some(caller.clone());
        if !is_cli && !is_tal {
            panic_with_error!(&env, Err::UnAuth);
        }

        if job.st != JobSt::Act {
            panic_with_error!(&env, Err::BadState);
        }

        if let Some(idx) = mile_idx {
            let mile = job.miles.get(idx)
                .unwrap_or_else(|| panic_with_error!(&env, Err::BadIdx));
                
            if mile.st != MileSt::Sub {
                panic_with_error!(&env, Err::NotSub);
            }
        }

        job.st = JobSt::Disp;
        job.disp_by = Some(caller.clone());
        Self::upd_job(&env, job_id, &job);
        
        env.events().publish(
            (DIS_RIS, caller),
            (job_id, mile_idx),
        );

        env.storage().remove(&RE_ENTRY);
    }

    pub fn resolve_disp(
        env: Env,
        arb: Address,
        job_id: u32,
        mile_idx: Option<u32>,
        dec: bool,
    ) {
        arb.require_auth();
        Self::chk_reent(&env);
        
        let mut job = Self::get_job(&env, job_id);

        if job.st != JobSt::Disp {
            panic_with_error!(&env, Err::BadState);
        }

        if dec {
            if let Some(idx) = mile_idx {
                Self::apr_one(&env, &mut job, idx);
            } else {
                Self::apr_all(&env, &mut job);
            }
        } else {
            if let Some(idx) = mile_idx {
                Self::rej_one(&env, &mut job, idx);
            } else {
                Self::rej_all(&env, &mut job);
            }
        }

        job.st = if job.miles.iter().all(|m| matches!(m.st, MileSt::Apr)) {
            JobSt::Done
        } else {
            JobSt::Act
        };

        job.disp_by = None;
        Self::upd_job(&env, job_id, &job);
        
        env.events().publish(
            (DIS_RES, arb),
            (job_id, mile_idx, dec),
        );

        env.storage().remove(&RE_ENTRY);
    }

    fn apr_one(env: &Env, job: &mut Job, idx: u32) {
        let mut mile = job.miles.get(idx)
            .unwrap_or_else(|| panic_with_error!(env, Err::BadIdx));
            
        mile.st = MileSt::Apr;
        job.miles.set(idx, mile);
        job.paid += mile.amt;
    }

    // fn apr_all(env: &Env, job: &mut Job) {
    //     for i in 0..job.miles.len() {
    //         let mut mile = job.miles.get(i).unwrap();
    //         if matches!(mile.st, MileSt::Sub) {
    //             mile.st = MileSt::Apr;
    //             job.miles.set(i, mile);
    //             job.paid += mile.amt;
    //         }
    //     }
    // }
    fn apr_all(env: &Env, job: &mut Job) {
        for i in 0..job.miles.len() {
            let mut mile = job.miles.get(i).unwrap();
            if matches!(mile.st, MileSt::Sub) {
                job.paid += mile.amt; // ✅ FIRST use the data
                mile.st = MileSt::Apr; // ✅ Then modify
                job.miles.set(i, mile); // ✅ LASTLY move it
            }
        }
    }
    

    fn rej_one(env: &Env, job: &mut Job, idx: u32) {
        let mut mile = job.miles.get(idx)
            .unwrap_or_else(|| panic_with_error!(env, Err::BadIdx));
            
        mile.st = MileSt::Pen;
        mile.data = BytesN::from_array(env, &[0; 32]);
        job.miles.set(idx, mile);
    }

    // fn rej_all(env: &Env, job: &mut Job) {
    //     for i in 0..job.miles.len() {
    //         let mut mile = job.miles.get(i).unwrap();
    //         if matches!(mile.st, MileSt::Sub) {
    //             mile.st = MileSt::Pen;
    //             mile.data = BytesN::from_array(env, &[0; 32]);
    //             job.miles.set(i, mile);
    //         }
    //     }
    // }
    fn rej_all(env: &Env, job: &mut Job) {
        for i in 0..job.miles.len() {
            let mut mile = job.miles.get(i).unwrap();
            if matches!(mile.st, MileSt::Sub) {
                mile.data = BytesN::from_array(env, &[0; 32]);
                mile.st = MileSt::Pen;
                job.miles.set(i, mile);
            }
        }
    }
    

    fn chk_reent(env: &Env) {
        if env.storage().has(&RE_ENTRY) {
            panic_with_error!(env, Err::ReEnt);
        }
        env.storage().set(&RE_ENTRY, &true);
    }

    fn save_job(env: &Env, job: &Job) -> u32 {
        let mut cnt = env.storage().get(&symbol_short!("JOB_CNT"))
            .unwrap_or(Ok(0u32))
            .unwrap();
        cnt += 1;
        env.storage().set(&symbol_short!("JOB_CNT"), &cnt);
        env.storage().set(&Self::job_key(cnt), job);
        cnt
    }

    fn upd_job(env: &Env, job_id: u32, job: &Job) {
        env.storage().set(&Self::job_key(job_id), job);
    }

    fn get_job(env: &Env, job_id: u32) -> Job {
        env.storage()
            .get(&Self::job_key(job_id))
            .unwrap_or_else(|| panic_with_error!(env, Err::NoJob))
            .unwrap()
    }

    fn job_key(job_id: u32) -> BytesN<32> {
        BytesN::from_array(&Env::default(), &{
            let mut arr = [0u8; 32];
            arr[..4].copy_from_slice(&job_id.to_be_bytes());
            arr
        })
    }
}