# AcadChain

## Project Description

AcadChain is a decentralized academic knowledge marketplace built on Soroban and the Stellar blockchain. It enables researchers, students, and academic authors to securely publish, verify, and monetize academic resources such as theses, research papers, technical reports, and educational datasets.

The platform combines AI-powered validation and human reviewer approval to ensure content quality and authenticity. Users can purchase permanent access to verified academic materials using XLM, while royalties are automatically distributed to authors, reviewers, and the platform through transparent smart contracts.

---

## Project Vision

AcadChain aims to build a decentralized global academic ecosystem where:

* knowledge ownership is protected,
* academic materials are fairly monetized,
* verification is transparent and trustless,
* educational resources are more accessible worldwide.

By leveraging blockchain technology, AcadChain eliminates centralized control while providing immutable ownership proof and automated royalty distribution.

---

## Key Features

### Academic Document Registration

Authors can publish:

* Theses
* Dissertations
* Research Papers
* Technical Reports
* Educational Datasets

Only metadata and encrypted IPFS CIDs are stored on-chain to ensure transparency and copyright protection.

### Dual Verification System

Documents undergo a two-step verification process:

**AI Verification**

* plagiarism detection,
* content validation,
* metadata consistency check,
* AI-generated reports.

**Human Reviewer Approval**

Registered reviewers vote to:

* Approve
* Reject

Documents are published only after reaching the required approval quorum.

### Reviewer Staking

Reviewers stake XLM to participate.

This mechanism:

* increases accountability,
* discourages malicious reviews,
* rewards honest reviewers,
* enables slashing for fraudulent behavior.

### Permanent Access Licensing

Users purchase permanent access to published documents using XLM.

Access rights are stored transparently on-chain.

### Automated Royalty Distribution

Revenue from each purchase is automatically distributed:

```txt
70% → Author
20% → Platform Treasury
10% → Reviewer Reward Pool
```

### Copyright Protection

AcadChain records cryptographic fingerprints of documents on-chain, enabling:

* ownership proof,
* timestamp verification,
* plagiarism evidence,
* copyright dispute support.

### Transparent Lifecycle

Every document follows a transparent lifecycle:

```txt
SUBMITTED
    ↓
AI VERIFICATION
    ↓
PENDING REVIEW
    ↓
UNDER REVIEW
    ↓
PUBLISHED / REJECTED
```

All state transitions are permanently recorded on-chain.

### Admin Controls

Administrators can:

* pause the protocol,
* revoke malicious content,
* slash fraudulent reviewers,
* update system configurations.

---

## Usage Instructions

1. Deploy the smart contract on Soroban.

2. Initialize:

* Admin wallet
* Treasury wallet
* XLM asset contract
* Minimum reviewer stake

3. Authors submit:

* title,
* abstract,
* encrypted IPFS CID,
* institution,
* language,
* access price.

4. AI performs verification.

5. Reviewers stake XLM and vote.

6. Published documents become available for purchase.

7. Users purchase permanent access using XLM.

8. Authors, reviewers, and treasury withdraw earnings.

---

## Future Scope

* Semantic academic search
* AI-powered document chat
* Automatic summarization
* Citation generation
* NFT academic certificates
* Institution verification
* Cross-university research marketplace
* Subscription-based premium plans
* DAO governance
* Scholar ecosystem token

---

## Technology Stack

### Blockchain

* Stellar Blockchain
* Soroban Smart Contracts

### Development

* Rust
* Soroban SDK

### Storage

* IPFS (Encrypted Files)

### Payments

* XLM

### Security

* Cryptographic ownership proof
* Immutable audit trail
* Reviewer staking
* Permissioned authorization

---

## Contribution

AcadChain welcomes contributions from:

* Blockchain developers
* Researchers
* Academic institutions
* AI engineers
* Soroban ecosystem contributors

Fork the repository and submit pull requests to help improve the decentralized academic ecosystem.

---

## License

This project is licensed under the MIT License.

---

### Contract Details

**Network:** Stellar Soroban

**Payment Token:** XLM

**License Model:** Permanent Access

**Verification Model:** AI + Human Review

**Royalty Distribution:**

```txt
70% Author
20% Platform
10% Reviewer Pool
```

**Smart Contract Language:** Rust + Soroban SDK
