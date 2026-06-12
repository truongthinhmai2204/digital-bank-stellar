//! # AcadChain — Ngân hàng Dữ liệu Số Học thuật
//!
//! Smart contract viết trên Soroban (soroban-sdk = 21) cho phép:
//!   - Tác giả đăng ký tài liệu học thuật (luận văn, đồ án, v.v.)
//!   - Kiểm chứng 2 lớp: AI (ghi nhận bởi admin) + Reviewer con người (stake XLM)
//!   - Người đọc trả phí XLM để truy cập
//!   - Royalty tự động: 70% tác giả / 20% platform / 10% reviewer pool
//!   - Toàn bộ state transition được emit event để off-chain indexer theo dõi
//!
//! ## Document Lifecycle
//! ```
//! SUBMITTED
//!   ──[AI pass, similarity ≤ 30%]──▶ PENDING_REVIEW
//!   ──[AI fail / similarity > 30%]──▶ REJECTED_BY_AI
//!
//! PENDING_REVIEW
//!   ──[first vote comes in]──▶ UNDER_REVIEW
//!
//! UNDER_REVIEW
//!   ──[≥2 approve & >50% approval]──▶ PUBLISHED
//!   ──[majority reject]────────────▶ REJECTED_BY_REVIEWERS
//!
//! PUBLISHED
//!   ──[admin action]──▶ REVOKED
//! ```

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short,
    token,
    Address, Env, String, Vec,
};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Minimum access price: 0.1 XLM (in stroops, 1 XLM = 10_000_000 stroops)
const MIN_ACCESS_PRICE: i128 = 1_000_000;

/// Default minimum reviewer stake: 1 XLM
const DEFAULT_MIN_STAKE: i128 = 10_000_000;

/// Access duration: ~30 days at ~5s/ledger = 518_400 ledgers
const ACCESS_TTL_LEDGERS: u32 = 518_400;

/// Maximum plagiarism similarity allowed to pass AI check (30%)
const MAX_SIMILARITY: u32 = 30;

/// Minimum reviewer approvals required for quorum
const MIN_APPROVALS: u32 = 2;

/// Royalty splits in basis points (sum = 10_000)
const AUTHOR_BPS: i128 = 7_000;   // 70%
const PLATFORM_BPS: i128 = 2_000; // 20%
const REVIEWER_BPS: i128 = 1_000; // 10%
const BPS_DENOM: i128 = 10_000;

// =============================================================================
// ERRORS
// =============================================================================

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // Init
    AlreadyInitialized      = 1,
    NotInitialized          = 2,
    // Auth / platform
    Unauthorized            = 10,
    ContractPaused          = 11,
    // Document
    DocumentNotFound        = 20,
    InvalidAccessPrice      = 21,
    TitleTooLong            = 22,
    AbstractTooLong         = 23,
    InvalidIpfsHash         = 24,
    DocumentNotPublished    = 25,
    NotReadyToFinalize      = 26,
    // AI verification
    NotPendingAiVerification = 30,
    InvalidSimilarityScore   = 31,
    // Reviewer
    ReviewerAlreadyExists   = 40,
    ReviewerNotFound        = 41,
    InsufficientStake       = 42,
    AlreadyReviewed         = 43,
    CannotReviewOwnDoc      = 44,
    NotPendingReview        = 45,
    InvalidQualityScore     = 46,
    ReviewerSlashed         = 47,
    // Payment
    AlreadyHasAccess        = 50,
    NoEarnings              = 51,
    // General
    InvalidInput            = 90,
}

// =============================================================================
// TYPES
// =============================================================================

/// Document types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocType {
    Thesis,          // Luận văn đại học / thạc sĩ
    Dissertation,    // Luận án tiến sĩ
    ResearchPaper,   // Bài báo nghiên cứu
    TechnicalReport, // Báo cáo kỹ thuật / white paper
}

/// Document lifecycle status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocStatus {
    PendingAiVerification,
    RejectedByAi,
    PendingReview,
    UnderReview,
    RejectedByReviewers,
    Published,
    Revoked,
}

/// Reviewer decision
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    Approve,
    Reject,
}

/// Full document metadata stored on-chain.
/// Actual file content lives on IPFS — only the CIDv1 hash is stored here.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Document {
    pub id: u64,
    pub author: Address,
    pub title: String,
    pub abstract_: String,
    /// IPFS CIDv1 of the encrypted file
    pub ipfs_hash: String,
    pub doc_type: DocType,
    /// Price in stroops readers must pay
    pub access_price: i128,
    /// ISO 639-1 language code (e.g. "vi", "en")
    pub language: String,
    pub institution: String,
    pub status: DocStatus,
    /// Ledger sequence at submission
    pub submitted_at: u32,
    /// Ledger sequence when published (0 = not yet)
    pub published_at: u32,
    /// Plagiarism similarity score 0–100 (255 = not checked yet)
    pub similarity_score: u32,
    /// IPFS hash of the AI analysis report
    pub ai_report_hash: String,
    pub approve_votes: u32,
    pub reject_votes: u32,
    /// Cumulative XLM earned (stroops)
    pub total_earned: i128,
    /// Total unique accesses sold
    pub access_count: u64,
}

/// Reviewer profile
#[contracttype]
#[derive(Clone, Debug)]
pub struct Reviewer {
    pub address: Address,
    /// XLM staked in stroops
    pub stake: i128,
    /// Comma-separated expertise tags
    pub expertise: String,
    pub review_count: u64,
    pub total_quality_given: u64,
    pub successful_approvals: u64,
    /// Reputation 0–100
    pub reputation: u32,
    pub slashed: bool,
    pub registered_at: u32,
}

/// Single review record
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReviewRecord {
    pub reviewer: Address,
    pub doc_id: u64,
    pub decision: Decision,
    pub comments: String,
    pub quality_score: u32,
    pub submitted_at: u32,
}

// =============================================================================
// STORAGE KEYS
// =============================================================================

#[contracttype]
#[derive(Clone)]
enum Key {
    // --- Instance (platform config) ---
    Admin,
    Treasury,
    XlmSac,         // XLM Stellar Asset Contract address
    MinStake,
    Paused,
    DocCount,

    // --- Persistent (per-entity) ---
    Doc(u64),
    DocReviewers(u64),  // Vec<Address> of reviewers who voted
    Rev(Address),       // Reviewer profile
    RevExists(Address),
    ReviewRecord(Address, u64),
    AccessExpiry(Address, u64), // expiry ledger for (reader, doc)
    Earnings(Address),
}

// =============================================================================
// CONTRACT
// =============================================================================

#[contract]
pub struct AcadChain;

#[contractimpl]
impl AcadChain {

    // =========================================================================
    // INITIALIZATION
    // =========================================================================

    /// Deploy the contract. Must be called once after deployment.
    ///
    /// # Arguments
    /// * `admin`      - Platform admin address (can record AI results, slash, pause)
    /// * `treasury`   - Platform wallet receiving 20% of every sale
    /// * `xlm_sac`    - XLM Stellar Asset Contract address
    ///                  Testnet: CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCN
    ///                  Mainnet: CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
    /// * `min_stake`  - Minimum XLM a reviewer must stake (stroops)
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        xlm_sac: Address,
        min_stake: i128,
    ) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Treasury, &treasury);
        env.storage().instance().set(&Key::XlmSac, &xlm_sac);
        env.storage().instance().set(&Key::MinStake, &min_stake);
        env.storage().instance().set(&Key::Paused, &false);
        env.storage().instance().set(&Key::DocCount, &0u64);

        env.events().publish((symbol_short!("INIT"),), (admin,));
    }

    // =========================================================================
    // DOCUMENT SUBMISSION
    // =========================================================================

    /// Author submits a new document for verification and review.
    ///
    /// Returns the new document ID (auto-incremented u64, starts at 1).
    ///
    /// # Arguments
    /// * `author`       - Author's Stellar address (must sign)
    /// * `title`        - Document title (max 256 chars)
    /// * `abstract_`    - Short abstract (max 1024 chars)
    /// * `ipfs_hash`    - CIDv1 hash of the encrypted file on IPFS (min 10 chars)
    /// * `doc_type`     - 0=Thesis, 1=Dissertation, 2=ResearchPaper, 3=TechnicalReport
    /// * `access_price` - XLM price in stroops readers must pay (min 1_000_000 = 0.1 XLM)
    /// * `language`     - ISO 639-1 code ("vi", "en", etc.)
    /// * `institution`  - Author's institution name
    pub fn submit_document(
        env: Env,
        author: Address,
        title: String,
        abstract_: String,
        ipfs_hash: String,
        doc_type: u32,
        access_price: i128,
        language: String,
        institution: String,
    ) -> u64 {
        Self::require_not_paused(&env);
        author.require_auth();

        if title.len() > 256 {
            panic_with_error!(&env, Error::TitleTooLong);
        }
        if abstract_.len() > 1024 {
            panic_with_error!(&env, Error::AbstractTooLong);
        }
        if ipfs_hash.len() < 10 {
            panic_with_error!(&env, Error::InvalidIpfsHash);
        }
        if access_price < MIN_ACCESS_PRICE {
            panic_with_error!(&env, Error::InvalidAccessPrice);
        }

        let doc_id = Self::next_doc_id(&env);

        let doc = Document {
            id: doc_id,
            author: author.clone(),
            title: title.clone(),
            abstract_,
            ipfs_hash,
            doc_type: Self::u32_to_doc_type(doc_type),
            access_price,
            language,
            institution,
            status: DocStatus::PendingAiVerification,
            submitted_at: env.ledger().sequence(),
            published_at: 0,
            similarity_score: 255, // 255 = sentinel "not yet checked"
            ai_report_hash: String::from_str(&env, ""),
            approve_votes: 0,
            reject_votes: 0,
            total_earned: 0,
            access_count: 0,
        };

        env.storage().persistent().set(&Key::Doc(doc_id), &doc);

        // Initialize empty reviewer list for this document
        let empty_reviewers: Vec<Address> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&Key::DocReviewers(doc_id), &empty_reviewers);

        env.events()
            .publish((symbol_short!("DOC_SUB"), doc_id), (author, title));

        doc_id
    }

    // =========================================================================
    // AI VERIFICATION
    // =========================================================================

    /// Record AI verification result for a document.
    /// Only callable by the platform admin.
    ///
    /// If `passed` is true AND `similarity_score` ≤ 30%, document moves to PendingReview.
    /// Otherwise it is marked RejectedByAi.
    ///
    /// # Arguments
    /// * `doc_id`           - Target document
    /// * `passed`           - Whether AI content check passed
    /// * `similarity_score` - Plagiarism similarity 0–100
    /// * `ai_report_hash`   - IPFS hash of the full AI report
    pub fn record_ai_verification(
        env: Env,
        doc_id: u64,
        passed: bool,
        similarity_score: u32,
        ai_report_hash: String,
    ) {
        Self::require_admin(&env);

        if similarity_score > 100 {
            panic_with_error!(&env, Error::InvalidSimilarityScore);
        }

        let mut doc = Self::load_doc(&env, doc_id);

        if doc.status != DocStatus::PendingAiVerification {
            panic_with_error!(&env, Error::NotPendingAiVerification);
        }

        doc.similarity_score = similarity_score;
        doc.ai_report_hash = ai_report_hash;

        if passed && similarity_score <= MAX_SIMILARITY {
            doc.status = DocStatus::PendingReview;
            env.events()
                .publish((symbol_short!("AI_PASS"), doc_id), (similarity_score,));
        } else {
            doc.status = DocStatus::RejectedByAi;
            env.events()
                .publish((symbol_short!("AI_FAIL"), doc_id), (similarity_score,));
        }

        env.storage().persistent().set(&Key::Doc(doc_id), &doc);
    }

    // =========================================================================
    // REVIEWER REGISTRATION
    // =========================================================================

    /// Register as a reviewer by staking XLM.
    ///
    /// XLM stake is transferred from the reviewer to this contract.
    /// Reviewers earn a share of the 10% reviewer pool from every document they approve.
    ///
    /// # Arguments
    /// * `reviewer`     - Reviewer's Stellar address (must sign)
    /// * `stake_amount` - XLM to stake in stroops (must meet minimum set by admin)
    /// * `expertise`    - Comma-separated expertise tags (e.g. "blockchain,computer-science")
    pub fn register_reviewer(
        env: Env,
        reviewer: Address,
        stake_amount: i128,
        expertise: String,
    ) {
        Self::require_not_paused(&env);
        reviewer.require_auth();

        if env
            .storage()
            .persistent()
            .has(&Key::RevExists(reviewer.clone()))
        {
            panic_with_error!(&env, Error::ReviewerAlreadyExists);
        }

        let min_stake: i128 = env
            .storage()
            .instance()
            .get(&Key::MinStake)
            .unwrap_or(DEFAULT_MIN_STAKE);

        if stake_amount < min_stake {
            panic_with_error!(&env, Error::InsufficientStake);
        }

        // Transfer XLM stake: reviewer → contract
        let xlm_sac: Address = env.storage().instance().get(&Key::XlmSac).unwrap();
        let token_client = token::TokenClient::new(&env, &xlm_sac);
        token_client.transfer(
            &reviewer,
            &env.current_contract_address(),
            &stake_amount,
        );

        let profile = Reviewer {
            address: reviewer.clone(),
            stake: stake_amount,
            expertise,
            review_count: 0,
            total_quality_given: 0,
            successful_approvals: 0,
            reputation: 50, // Neutral starting reputation
            slashed: false,
            registered_at: env.ledger().sequence(),
        };

        env.storage()
            .persistent()
            .set(&Key::Rev(reviewer.clone()), &profile);
        env.storage()
            .persistent()
            .set(&Key::RevExists(reviewer.clone()), &true);

        env.events()
            .publish((symbol_short!("REV_REG"),), (reviewer, stake_amount));
    }

    // =========================================================================
    // REVIEW SUBMISSION
    // =========================================================================

    /// Submit a review decision on a document.
    ///
    /// A reviewer can only vote once per document and cannot review their own work.
    /// After each vote, if quorum is met the document is auto-finalized.
    ///
    /// # Arguments
    /// * `reviewer`      - Reviewer's Stellar address (must sign)
    /// * `doc_id`        - Document to review
    /// * `decision`      - Approve or Reject
    /// * `comments`      - Public review comments (max 2048 chars)
    /// * `quality_score` - Quality rating 1–10
    pub fn review_document(
        env: Env,
        reviewer: Address,
        doc_id: u64,
        decision: Decision,
        comments: String,
        quality_score: u32,
    ) {
        Self::require_not_paused(&env);
        reviewer.require_auth();

        // Load and validate reviewer
        let mut rev_profile = Self::load_reviewer(&env, &reviewer);
        if rev_profile.slashed {
            panic_with_error!(&env, Error::ReviewerSlashed);
        }
        if quality_score < 1 || quality_score > 10 {
            panic_with_error!(&env, Error::InvalidQualityScore);
        }

        // Load and validate document
        let mut doc = Self::load_doc(&env, doc_id);
        if doc.author == reviewer {
            panic_with_error!(&env, Error::CannotReviewOwnDoc);
        }
        if doc.status != DocStatus::PendingReview && doc.status != DocStatus::UnderReview {
            panic_with_error!(&env, Error::NotPendingReview);
        }

        // Prevent double voting
        if env
            .storage()
            .persistent()
            .has(&Key::ReviewRecord(reviewer.clone(), doc_id))
        {
            panic_with_error!(&env, Error::AlreadyReviewed);
        }

        // Record vote
        let approved = decision == Decision::Approve;
        if approved {
            doc.approve_votes += 1;
        } else {
            doc.reject_votes += 1;
        }
        if doc.status == DocStatus::PendingReview {
            doc.status = DocStatus::UnderReview;
        }
        env.storage().persistent().set(&Key::Doc(doc_id), &doc);

        // Add reviewer to document's reviewer list
        let mut doc_reviewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&Key::DocReviewers(doc_id))
            .unwrap_or_else(|| Vec::new(&env));
        doc_reviewers.push_back(reviewer.clone());
        env.storage()
            .persistent()
            .set(&Key::DocReviewers(doc_id), &doc_reviewers);

        // Store review record
        let record = ReviewRecord {
            reviewer: reviewer.clone(),
            doc_id,
            decision,
            comments,
            quality_score,
            submitted_at: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&Key::ReviewRecord(reviewer.clone(), doc_id), &record);

        // Update reviewer stats & reputation
        rev_profile.review_count += 1;
        rev_profile.total_quality_given += quality_score as u64;
        Self::recompute_reputation(&mut rev_profile);
        env.storage()
            .persistent()
            .set(&Key::Rev(reviewer.clone()), &rev_profile);

        env.events().publish(
            (symbol_short!("REVIEW"), doc_id),
            (reviewer, approved, quality_score),
        );

        // Auto-finalize if quorum reached (total votes ≥ MIN_APPROVALS * 2)
        let updated_doc = Self::load_doc(&env, doc_id);
        let total = updated_doc.approve_votes + updated_doc.reject_votes;
        if total >= MIN_APPROVALS * 2 {
            Self::do_finalize(&env, doc_id);
        }
    }

    // =========================================================================
    // FINALIZE DOCUMENT
    // =========================================================================

    /// Manually trigger finalization of a document after sufficient votes.
    /// Can be called by anyone; auto-triggered after every review if quorum met.
    ///
    /// Approval quorum: ≥ MIN_APPROVALS (2) approve votes AND > 50% of total votes.
    pub fn finalize_document(env: Env, doc_id: u64) {
        Self::do_finalize(&env, doc_id);
    }

    // =========================================================================
    // PURCHASE ACCESS
    // =========================================================================

    /// Pay to read a published document.
    ///
    /// Payment is split immediately:
    ///   - 70% credited to author's pending earnings
    ///   - 20% credited to platform treasury's pending earnings
    ///   - 10% split evenly among the document's reviewers
    ///
    /// Returns the expiry ledger number (access valid for ~30 days).
    ///
    /// # Arguments
    /// * `reader` - Reader's Stellar address (must sign)
    /// * `doc_id` - Document to access
    pub fn purchase_access(env: Env, reader: Address, doc_id: u64) -> u64 {
        Self::require_not_paused(&env);
        reader.require_auth();

        let mut doc = Self::load_doc(&env, doc_id);

        if doc.status != DocStatus::Published {
            panic_with_error!(&env, Error::DocumentNotPublished);
        }

        // Check for existing valid access
        let current_expiry: u32 = env
            .storage()
            .persistent()
            .get(&Key::AccessExpiry(reader.clone(), doc_id))
            .unwrap_or(0u32);
        if current_expiry > env.ledger().sequence() {
            panic_with_error!(&env, Error::AlreadyHasAccess);
        }

        let price = doc.access_price;

        // Transfer XLM: reader → contract
        let xlm_sac: Address = env.storage().instance().get(&Key::XlmSac).unwrap();
        let token_client = token::TokenClient::new(&env, &xlm_sac);
        token_client.transfer(&reader, &env.current_contract_address(), &price);

        // Compute splits
        let author_share   = (price * AUTHOR_BPS)   / BPS_DENOM;
        let platform_share = (price * PLATFORM_BPS) / BPS_DENOM;
        let reviewer_pool  = price - author_share - platform_share;

        // Credit author
        Self::add_earnings(&env, &doc.author, author_share);

        // Credit platform treasury
        let treasury: Address = env.storage().instance().get(&Key::Treasury).unwrap();
        Self::add_earnings(&env, &treasury, platform_share);

        // Distribute reviewer pool evenly
        let doc_reviewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&Key::DocReviewers(doc_id))
            .unwrap_or_else(|| Vec::new(&env));

        let n = doc_reviewers.len() as i128;
        if n > 0 {
            let per_reviewer = reviewer_pool / n;
            let mut distributed: i128 = 0;
            for rev in doc_reviewers.iter() {
                Self::add_earnings(&env, &rev, per_reviewer);
                distributed += per_reviewer;
            }
            // Rounding remainder → treasury
            let remainder = reviewer_pool - distributed;
            if remainder > 0 {
                Self::add_earnings(&env, &treasury, remainder);
            }
        } else {
            // No reviewers yet → all reviewer pool goes to treasury
            Self::add_earnings(&env, &treasury, reviewer_pool);
        }

        // Record access expiry
        let expiry = env.ledger().sequence() + ACCESS_TTL_LEDGERS;
        env.storage()
            .persistent()
            .set(&Key::AccessExpiry(reader.clone(), doc_id), &expiry);

        // Update document stats
        doc.total_earned = doc.total_earned.saturating_add(price);
        doc.access_count += 1;
        env.storage().persistent().set(&Key::Doc(doc_id), &doc);

        env.events()
            .publish((symbol_short!("ACCESS"), doc_id), (reader, price, expiry));

        expiry as u64
    }

    // =========================================================================
    // WITHDRAW EARNINGS
    // =========================================================================

    /// Withdraw all pending XLM earnings for the caller.
    ///
    /// Works for authors, reviewers, and the platform treasury.
    /// Balance is cleared before transfer (reentrancy guard).
    pub fn withdraw_earnings(env: Env, beneficiary: Address) {
        beneficiary.require_auth();

        let balance: i128 = env
            .storage()
            .persistent()
            .get(&Key::Earnings(beneficiary.clone()))
            .unwrap_or(0i128);

        if balance <= 0 {
            panic_with_error!(&env, Error::NoEarnings);
        }

        // Clear balance BEFORE transfer (reentrancy guard)
        env.storage()
            .persistent()
            .remove(&Key::Earnings(beneficiary.clone()));

        // Transfer XLM: contract → beneficiary
        let xlm_sac: Address = env.storage().instance().get(&Key::XlmSac).unwrap();
        let token_client = token::TokenClient::new(&env, &xlm_sac);
        token_client.transfer(&env.current_contract_address(), &beneficiary, &balance);

        env.events()
            .publish((symbol_short!("WITHDRAW"),), (beneficiary, balance));
    }

    // =========================================================================
    // QUERIES
    // =========================================================================

    /// Get full document metadata.
    pub fn get_document(env: Env, doc_id: u64) -> Document {
        Self::load_doc(&env, doc_id)
    }

    /// Get current document status.
    pub fn get_document_status(env: Env, doc_id: u64) -> DocStatus {
        Self::load_doc(&env, doc_id).status
    }

    /// Get total number of documents submitted.
    pub fn get_document_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&Key::DocCount)
            .unwrap_or(0u64)
    }

    /// Check whether a reader has active access to a document.
    pub fn has_access(env: Env, reader: Address, doc_id: u64) -> bool {
        let expiry: u32 = env
            .storage()
            .persistent()
            .get(&Key::AccessExpiry(reader, doc_id))
            .unwrap_or(0u32);
        expiry > env.ledger().sequence()
    }

    /// Get pending XLM earnings (in stroops) for an address.
    pub fn get_earnings(env: Env, address: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::Earnings(address))
            .unwrap_or(0i128)
    }

    /// Get a reviewer's profile.
    pub fn get_reviewer(env: Env, reviewer: Address) -> Reviewer {
        Self::load_reviewer(&env, &reviewer)
    }

    /// Get the review record submitted by a reviewer for a specific document.
    pub fn get_review_record(env: Env, reviewer: Address, doc_id: u64) -> ReviewRecord {
        env.storage()
            .persistent()
            .get(&Key::ReviewRecord(reviewer, doc_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::ReviewerNotFound))
    }

    // =========================================================================
    // ADMIN FUNCTIONS
    // =========================================================================

    /// Update the platform treasury address.
    pub fn set_treasury(env: Env, new_treasury: Address) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&Key::Treasury, &new_treasury);
    }

    /// Update minimum reviewer stake requirement.
    pub fn set_min_stake(env: Env, new_min_stake: i128) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&Key::MinStake, &new_min_stake);
    }

    /// Revoke a published document (DMCA / policy violation).
    pub fn revoke_document(env: Env, doc_id: u64) {
        Self::require_admin(&env);
        let mut doc = Self::load_doc(&env, doc_id);
        doc.status = DocStatus::Revoked;
        env.storage().persistent().set(&Key::Doc(doc_id), &doc);
        env.events()
            .publish((symbol_short!("REVOKED"), doc_id), ());
    }

    /// Slash a reviewer for malicious behavior.
    /// Slashed reviewer's stake is forfeited to the platform treasury.
    pub fn slash_reviewer(env: Env, reviewer: Address, reason: String) {
        Self::require_admin(&env);

        let mut rev = Self::load_reviewer(&env, &reviewer);
        let forfeited_stake = rev.stake;

        rev.slashed = true;
        rev.stake = 0;
        env.storage()
            .persistent()
            .set(&Key::Rev(reviewer.clone()), &rev);

        // Forfeited stake goes to treasury
        if forfeited_stake > 0 {
            let treasury: Address = env.storage().instance().get(&Key::Treasury).unwrap();
            Self::add_earnings(&env, &treasury, forfeited_stake);
        }

        env.events()
            .publish((symbol_short!("SLASHED"),), (reviewer, reason));
    }

    /// Pause all operations (emergency stop).
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&Key::Paused, &true);
        env.events().publish((symbol_short!("PAUSED"),), ());
    }

    /// Resume operations after a pause.
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&Key::Paused, &false);
        env.events().publish((symbol_short!("UNPAUSED"),), ());
    }

    // =========================================================================
    // PRIVATE HELPERS
    // =========================================================================

    fn require_admin(env: &Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Key::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
        admin.require_auth();
    }

    fn require_not_paused(env: &Env) {
        let paused: bool = env
            .storage()
            .instance()
            .get(&Key::Paused)
            .unwrap_or(false);
        if paused {
            panic_with_error!(env, Error::ContractPaused);
        }
    }

    fn next_doc_id(env: &Env) -> u64 {
        let count: u64 = env
            .storage()
            .instance()
            .get(&Key::DocCount)
            .unwrap_or(0u64);
        let new_count = count + 1;
        env.storage()
            .instance()
            .set(&Key::DocCount, &new_count);
        new_count
    }

    fn load_doc(env: &Env, doc_id: u64) -> Document {
        env.storage()
            .persistent()
            .get(&Key::Doc(doc_id))
            .unwrap_or_else(|| panic_with_error!(env, Error::DocumentNotFound))
    }

    fn load_reviewer(env: &Env, reviewer: &Address) -> Reviewer {
        env.storage()
            .persistent()
            .get(&Key::Rev(reviewer.clone()))
            .unwrap_or_else(|| panic_with_error!(env, Error::ReviewerNotFound))
    }

    fn add_earnings(env: &Env, address: &Address, amount: i128) {
        let current: i128 = env
            .storage()
            .persistent()
            .get(&Key::Earnings(address.clone()))
            .unwrap_or(0i128);
        env.storage()
            .persistent()
            .set(&Key::Earnings(address.clone()), &(current.saturating_add(amount)));
    }

    fn do_finalize(env: &Env, doc_id: u64) {
        let mut doc = Self::load_doc(env, doc_id);

        if doc.status != DocStatus::UnderReview {
            panic_with_error!(env, Error::NotReadyToFinalize);
        }

        let total = doc.approve_votes + doc.reject_votes;
        if total < MIN_APPROVALS {
            panic_with_error!(env, Error::NotReadyToFinalize);
        }

        // Need ≥ MIN_APPROVALS approvals AND > 50% approval ratio
        let approval_bps = (doc.approve_votes as u64 * 10_000) / total as u64;

        if doc.approve_votes >= MIN_APPROVALS && approval_bps > 5_000 {
            doc.status = DocStatus::Published;
            doc.published_at = env.ledger().sequence();
            env.events()
                .publish((symbol_short!("DOC_PUB"), doc_id), (doc.author.clone(),));
        } else {
            doc.status = DocStatus::RejectedByReviewers;
            env.events()
                .publish((symbol_short!("DOC_REJ"), doc_id), ());
        }

        env.storage().persistent().set(&Key::Doc(doc_id), &doc);
    }

    /// Reputation formula:
    ///   base 50
    ///   + up to 30 pts from successful approval ratio
    ///   + up to 20 pts from average quality score
    fn recompute_reputation(rev: &mut Reviewer) {
        if rev.review_count == 0 {
            return;
        }
        let success_pts = (rev.successful_approvals * 30) / rev.review_count;
        let avg_quality = rev.total_quality_given / rev.review_count; // 1–10
        let quality_pts = (avg_quality * 20) / 10;
        rev.reputation = (50 + success_pts + quality_pts).min(100) as u32;
    }

    fn u32_to_doc_type(val: u32) -> DocType {
        match val {
            0 => DocType::Thesis,
            1 => DocType::Dissertation,
            2 => DocType::ResearchPaper,
            _ => DocType::TechnicalReport,
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String,
    };

    // -------------------------------------------------------------------------
    // Test helpers
    // -------------------------------------------------------------------------

    /// Sets up a fresh Env with the contract registered and initialized.
    /// Returns (env, client, admin, treasury, xlm_token_address).
    fn setup() -> (Env, AcadChainClient<'static>, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        // Register a mock XLM token contract so token transfers don't panic
        let xlm_admin = Address::generate(&env);
        let xlm_address = env.register_stellar_asset_contract_v2(xlm_admin.clone()).address();

        let contract_id = env.register(AcadChain, ());
        let client = AcadChainClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        client.initialize(&admin, &treasury, &xlm_address, &10_000_000i128);

        (env, client, admin, treasury, xlm_address)
    }

    fn sample_submit_args(env: &Env) -> (String, String, String, u32, i128, String, String) {
        (
            String::from_str(env, "Blockchain-Based Academic Verification System"),
            String::from_str(env, "This thesis explores decentralized approaches to verifying academic credentials using Stellar/Soroban smart contracts."),
            String::from_str(env, "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"),
            0u32,          // Thesis
            10_000_000i128, // 1 XLM
            String::from_str(env, "vi"),
            String::from_str(env, "Dai hoc Bach Khoa TP.HCM"),
        )
    }

    // -------------------------------------------------------------------------
    // Initialize
    // -------------------------------------------------------------------------

    #[test]
    fn test_initialize_ok() {
        let (_env, client, _admin, _treasury, _xlm) = setup();
        // If we got here without panic, initialize worked
        assert_eq!(client.get_document_count(), 0u64);
    }

    #[test]
    #[should_panic(expected = "AlreadyInitialized")]
    fn test_initialize_twice_panics() {
        let (env, client, admin, treasury, xlm) = setup();
        // Second call should panic
        client.initialize(&admin, &treasury, &xlm, &10_000_000i128);
        let _ = env;
    }

    // -------------------------------------------------------------------------
    // Submit document
    // -------------------------------------------------------------------------

    #[test]
    fn test_submit_document_ok() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (title, abs, ipfs, dt, price, lang, inst) = sample_submit_args(&env);

        let doc_id = client.submit_document(&author, &title, &abs, &ipfs, &dt, &price, &lang, &inst);

        assert_eq!(doc_id, 1u64);
        assert_eq!(client.get_document_count(), 1u64);

        let doc = client.get_document(&doc_id);
        assert_eq!(doc.author, author);
        assert_eq!(doc.status, DocStatus::PendingAiVerification);
        assert_eq!(doc.similarity_score, 255u32);
        assert_eq!(doc.approve_votes, 0u32);
        assert_eq!(doc.access_count, 0u64);
    }

    #[test]
    fn test_submit_multiple_documents_increments_id() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);

        let id1 = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        let id2 = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        let id3 = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);

        assert_eq!(id1, 1u64);
        assert_eq!(id2, 2u64);
        assert_eq!(id3, 3u64);
        assert_eq!(client.get_document_count(), 3u64);
    }

    #[test]
    #[should_panic]
    fn test_submit_price_too_low_panics() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (t, a, i, dt, _, l, ins) = sample_submit_args(&env);
        // 500_000 stroops = 0.05 XLM < minimum 0.1 XLM
        client.submit_document(&author, &t, &a, &i, &dt, &500_000i128, &l, &ins);
    }

    #[test]
    #[should_panic]
    fn test_submit_invalid_ipfs_panics() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (t, a, _, dt, p, l, ins) = sample_submit_args(&env);
        let bad_ipfs = String::from_str(&env, "bad"); // too short
        client.submit_document(&author, &t, &a, &bad_ipfs, &dt, &p, &l, &ins);
    }

    // -------------------------------------------------------------------------
    // AI Verification
    // -------------------------------------------------------------------------

    #[test]
    fn test_ai_verification_pass() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);

        client.record_ai_verification(
            &doc_id,
            &true,
            &15u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        let doc = client.get_document(&doc_id);
        assert_eq!(doc.status, DocStatus::PendingReview);
        assert_eq!(doc.similarity_score, 15u32);
    }

    #[test]
    fn test_ai_verification_fails_high_similarity() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);

        // 80% similarity — above 30% threshold
        client.record_ai_verification(
            &doc_id,
            &true,
            &80u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        assert_eq!(client.get_document_status(&doc_id), DocStatus::RejectedByAi);
    }

    #[test]
    fn test_ai_verification_explicit_fail() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);

        // passed=false even with low similarity
        client.record_ai_verification(
            &doc_id,
            &false,
            &5u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        assert_eq!(client.get_document_status(&doc_id), DocStatus::RejectedByAi);
    }

    // -------------------------------------------------------------------------
    // Reviewer registration
    // -------------------------------------------------------------------------

    #[test]
    fn test_register_reviewer_ok() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let reviewer = Address::generate(&env);

        client.register_reviewer(
            &reviewer,
            &10_000_000i128,
            &String::from_str(&env, "blockchain,smart-contracts"),
        );

        let profile = client.get_reviewer(&reviewer);
        assert_eq!(profile.stake, 10_000_000i128);
        assert_eq!(profile.reputation, 50u32);
        assert!(!profile.slashed);
        assert_eq!(profile.review_count, 0u64);
    }

    #[test]
    #[should_panic]
    fn test_register_reviewer_double_panics() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let reviewer = Address::generate(&env);
        let exp = String::from_str(&env, "math");

        client.register_reviewer(&reviewer, &10_000_000i128, &exp);
        client.register_reviewer(&reviewer, &10_000_000i128, &exp); // should panic
    }

    #[test]
    #[should_panic]
    fn test_register_reviewer_insufficient_stake_panics() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let reviewer = Address::generate(&env);
        // 5_000_000 stroops = 0.5 XLM < 1 XLM minimum
        client.register_reviewer(
            &reviewer,
            &5_000_000i128,
            &String::from_str(&env, "cs"),
        );
    }

    // -------------------------------------------------------------------------
    // Full happy path: submit → AI pass → 2 approvals → published
    // -------------------------------------------------------------------------

    #[test]
    fn test_full_lifecycle_published() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let rev1 = Address::generate(&env);
        let rev2 = Address::generate(&env);

        // 1. Submit
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);

        // 2. AI passes
        client.record_ai_verification(
            &doc_id,
            &true,
            &10u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );
        assert_eq!(client.get_document_status(&doc_id), DocStatus::PendingReview);

        // 3. Two reviewers register
        let exp = String::from_str(&env, "blockchain");
        client.register_reviewer(&rev1, &10_000_000i128, &exp);
        client.register_reviewer(&rev2, &10_000_000i128, &exp);

        // 4. Rev1 approves
        client.review_document(
            &rev1, &doc_id, &Decision::Approve,
            &String::from_str(&env, "Excellent methodology and clear results."),
            &9u32,
        );
        assert_eq!(client.get_document_status(&doc_id), DocStatus::UnderReview);

        // 5. Rev2 approves → quorum (2 approvals, 100% approval) → auto-finalize
        client.review_document(
            &rev2, &doc_id, &Decision::Approve,
            &String::from_str(&env, "Well-structured and contributes to the field."),
            &8u32,
        );

        // 6. Published!
        let doc = client.get_document(&doc_id);
        assert_eq!(doc.status, DocStatus::Published);
        assert_eq!(doc.approve_votes, 2u32);
        assert_eq!(doc.reject_votes, 0u32);
        assert_ne!(doc.published_at, 0u32);
    }

    // -------------------------------------------------------------------------
    // Rejection path
    // -------------------------------------------------------------------------

    #[test]
    fn test_lifecycle_rejected_by_reviewers() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let rev1 = Address::generate(&env);
        let rev2 = Address::generate(&env);
        let rev3 = Address::generate(&env);

        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        client.record_ai_verification(
            &doc_id, &true, &5u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        let exp = String::from_str(&env, "math");
        client.register_reviewer(&rev1, &10_000_000i128, &exp);
        client.register_reviewer(&rev2, &10_000_000i128, &exp);
        client.register_reviewer(&rev3, &10_000_000i128, &exp);

        let c = String::from_str(&env, "comment");
        client.review_document(&rev1, &doc_id, &Decision::Reject, &c, &3u32);
        client.review_document(&rev2, &doc_id, &Decision::Reject, &c, &4u32);
        client.review_document(&rev3, &doc_id, &Decision::Approve, &c, &7u32);

        // 1 approve, 2 reject → rejected
        assert_eq!(client.get_document_status(&doc_id), DocStatus::RejectedByReviewers);
    }

    // -------------------------------------------------------------------------
    // Cannot review own document
    // -------------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_author_cannot_review_own_doc() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);

        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        client.record_ai_verification(
            &doc_id, &true, &10u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        client.register_reviewer(&author, &10_000_000i128, &String::from_str(&env, "self"));
        client.review_document(
            &author, &doc_id, &Decision::Approve,
            &String::from_str(&env, "I approve myself."), &10u32,
        ); // should panic
    }

    // -------------------------------------------------------------------------
    // Cannot review twice
    // -------------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_double_review_panics() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let rev = Address::generate(&env);

        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        client.record_ai_verification(
            &doc_id, &true, &10u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        client.register_reviewer(&rev, &10_000_000i128, &String::from_str(&env, "cs"));
        let c = String::from_str(&env, "good");
        client.review_document(&rev, &doc_id, &Decision::Approve, &c, &8u32);
        client.review_document(&rev, &doc_id, &Decision::Approve, &c, &8u32); // panic
    }

    // -------------------------------------------------------------------------
    // Pause / Unpause
    // -------------------------------------------------------------------------

    #[test]
    #[should_panic]
    fn test_submit_while_paused_panics() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        client.pause();
        let author = Address::generate(&env);
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
    }

    #[test]
    fn test_submit_after_unpause_works() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        client.pause();
        client.unpause();
        let author = Address::generate(&env);
        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        assert_eq!(doc_id, 1u64);
    }

    // -------------------------------------------------------------------------
    // Slash reviewer
    // -------------------------------------------------------------------------

    #[test]
    fn test_slash_reviewer() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let rev = Address::generate(&env);

        client.register_reviewer(&rev, &10_000_000i128, &String::from_str(&env, "cs"));
        assert_eq!(client.get_reviewer(&rev).stake, 10_000_000i128);

        client.slash_reviewer(&rev, &String::from_str(&env, "Submitted fake reviews"));

        let profile = client.get_reviewer(&rev);
        assert!(profile.slashed);
        assert_eq!(profile.stake, 0i128);
    }

    #[test]
    #[should_panic]
    fn test_slashed_reviewer_cannot_review() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let rev = Address::generate(&env);

        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        client.record_ai_verification(
            &doc_id, &true, &10u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        client.register_reviewer(&rev, &10_000_000i128, &String::from_str(&env, "cs"));
        client.slash_reviewer(&rev, &String::from_str(&env, "cheater"));

        client.review_document(
            &rev, &doc_id, &Decision::Approve,
            &String::from_str(&env, "still trying"), &5u32,
        ); // should panic
    }

    // -------------------------------------------------------------------------
    // has_access returns false before purchase
    // -------------------------------------------------------------------------

    #[test]
    fn test_no_access_before_purchase() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let reader = Address::generate(&env);

        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);

        assert!(!client.has_access(&reader, &doc_id));
    }

    // -------------------------------------------------------------------------
    // Earnings accumulate and are readable
    // -------------------------------------------------------------------------

    #[test]
    fn test_earnings_start_at_zero() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let someone = Address::generate(&env);
        assert_eq!(client.get_earnings(&someone), 0i128);
    }

    // -------------------------------------------------------------------------
    // Revoke document
    // -------------------------------------------------------------------------

    #[test]
    fn test_revoke_document() {
        let (env, client, _admin, _treasury, _xlm) = setup();
        let author = Address::generate(&env);
        let rev1 = Address::generate(&env);
        let rev2 = Address::generate(&env);

        let (t, a, i, dt, p, l, ins) = sample_submit_args(&env);
        let doc_id = client.submit_document(&author, &t, &a, &i, &dt, &p, &l, &ins);
        client.record_ai_verification(
            &doc_id, &true, &5u32,
            &String::from_str(&env, "bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"),
        );

        let exp = String::from_str(&env, "cs");
        let c = String::from_str(&env, "ok");
        client.register_reviewer(&rev1, &10_000_000i128, &exp);
        client.register_reviewer(&rev2, &10_000_000i128, &exp);
        client.review_document(&rev1, &doc_id, &Decision::Approve, &c, &8u32);
        client.review_document(&rev2, &doc_id, &Decision::Approve, &c, &9u32);

        assert_eq!(client.get_document_status(&doc_id), DocStatus::Published);

        client.revoke_document(&doc_id);
        assert_eq!(client.get_document_status(&doc_id), DocStatus::Revoked);
    }
}