# AcadChain — Decentralized Academic Data Bank

## Project Title

AcadChain

---

## Project Description

AcadChain is a decentralized academic knowledge marketplace built on Soroban and the Stellar blockchain. It enables researchers, students, and academic authors to securely publish, verify, and monetize academic resources such as theses, dissertations, research papers, technical reports, and educational datasets.

The platform introduces a trustless verification model combining **AI-based validation** and **human reviewer approval**, ensuring academic quality and authenticity. Readers can purchase permanent access to verified academic content using XLM, while royalties are automatically distributed to authors, reviewers, and the platform through transparent smart contracts.

AcadChain transforms academic publishing into a decentralized, copyright-protected, and revenue-sharing ecosystem.

---

## Project Vision

The vision of AcadChain is to build a decentralized global academic ecosystem where knowledge ownership is protected, academic materials are fairly monetized, and educational access becomes more transparent and trustworthy.

Traditional academic sharing systems often suffer from:

* plagiarism risks,
* lack of verification,
* centralized control,
* unfair monetization,
* inaccessible research materials.

AcadChain solves these issues through blockchain-based ownership proof, decentralized verification, and automated royalty distribution powered by Soroban smart contracts.

---

## Key Features

### **Academic Document Registration**

Authors can upload and register academic materials including:

* Theses
* Dissertations
* Research Papers
* Technical Reports
* Educational datasets

Only metadata and encrypted file hashes (IPFS CID) are stored on-chain for transparency and copyright proof.

### **Dual Verification System**

AcadChain introduces a **2-layer verification process**:

#### **AI Verification**

Documents are checked through AI-based analysis:

* plagiarism similarity detection,
* content quality validation,
* metadata verification,
* AI-generated analysis reports.

#### **Human Reviewer Verification**

Verified reviewers assess academic quality and vote:

* Approve
* Reject

Documents require approval quorum before publication.

### **Reviewer Staking Mechanism**

Reviewers must stake XLM to participate.

This creates accountability and reduces malicious reviews.

Reviewers can be:

* rewarded for valid reviews,
* slashed for fraudulent or abusive behavior.

### **Permanent Paid Access**

Users can purchase **permanent access licenses** to published documents using XLM.

Access rights are recorded transparently on-chain.

### **Automated Royalty Distribution**

Every purchase automatically distributes revenue:

```txt
70% → Document Author
20% → Platform Treasury
10% → Reviewer Reward Pool
```

This ensures fair compensation without intermediaries.

### **Copyright & Ownership Protection**

AcadChain stores document cryptographic fingerprints on-chain.

This enables:

* proof of ownership,
* timestamp verification,
* anti-plagiarism evidence,
* copyright dispute support.

### **Transparent Lifecycle Tracking**

Each document follows a fully transparent lifecycle:

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

All state changes are recorded immutably on-chain.

### **Admin Security Controls**

Platform administrators can:

* pause the protocol,
* revoke malicious content,
* slash fraudulent reviewers,
* manage system configurations.

---

## Usage Instructions

### 1. Deploy Contract

Deploy the AcadChain smart contract on Soroban.

Initialize:

* Admin wallet
* Treasury wallet
* XLM asset contract
* Minimum reviewer stake

---

### 2. Submit Academic Documents

Authors register academic content:

* title
* abstract
* IPFS encrypted file hash
* institution
* language
* access price

Documents enter the **Pending AI Verification** stage.

---

### 3. AI Verification

Platform AI validates:

* plagiarism similarity score,
* content integrity,
* AI report generation.

Approved documents move to human review.

---

### 4. Reviewer Evaluation

Registered reviewers stake XLM and submit review votes.

A document becomes published when:

* minimum approval quorum is reached,
* approval ratio exceeds 50%.

---

### 5. Purchase Access

Readers pay in **XLM** to access published documents.

The smart contract:

* grants permanent access,
* records ownership,
* distributes royalties automatically.

---

### 6. Withdraw Earnings

Authors, reviewers, and treasury can withdraw accumulated XLM earnings at any time.

---

## Future Scope

### **Advanced AI Integration**

* semantic academic search,
* automatic summarization,
* AI-powered document chat,
* citation generation.

### **NFT-Based Academic Certificates**

Convert verified research ownership into academic NFTs.

### **Institution Verification**

Universities can verify official academic submissions.

### **Cross-University Research Marketplace**

Enable global collaboration among institutions and researchers.

### **Subscription-Based Premium Access**

Offer premium plans for unlimited research access.

### **Decentralized Governance**

Introduce DAO governance for moderation and protocol decisions.

### **Scholar Token Ecosystem**

Launch an ecosystem token for governance, rewards, and fee reductions.

---

## Technology Stack

### Blockchain Layer

* Soroban Smart Contracts
* Stellar Blockchain

### Smart Contract Development

* Rust
* Soroban SDK

### Storage Layer

* IPFS (Encrypted Academic Files)

### Payment System

* XLM (Stellar Lumens)

### Security

* Cryptographic ownership proof
* Immutable on-chain audit trail
* Reviewer staking mechanism
* Permissioned authorization

---

## Smart Contract Features

### Document Management

* Submit document
* Query metadata
* Publication tracking
* Copyright proof

### Verification System

* AI verification
* Human review voting
* Reputation logic
* Reviewer staking

### Access Management

* Permanent document access
* Access validation
* Immutable ownership

### Revenue Management

* Royalty split
* Earnings tracking
* Withdraw system

### Security Controls

* Emergency pause
* Reviewer slashing
* Document revocation

---

## Contribution

AcadChain welcomes contributions from:

* blockchain developers,
* researchers,
* academic institutions,
* AI engineers,
* Soroban ecosystem contributors.

Fork the repository and submit pull requests to improve the decentralized academic ecosystem.

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
