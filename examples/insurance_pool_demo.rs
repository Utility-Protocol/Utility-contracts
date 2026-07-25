// Insurance Pool Governance Demo
// This file demonstrates the key concepts and functionality of the Stream Insurance Pool system

use std::collections::HashMap;

// Simplified structs for demonstration
#[derive(Debug, Clone)]
struct InsurancePool {
    total_funds: i128,
    total_members: u32,
    base_premium_rate_bps: i128,
    is_active: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PoolMember {
    user_id: String,
    premium_paid: i128,
    risk_score: u32,
    claim_count: u32,
    is_active: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Claim {
    claim_id: u64,
    claimant: String,
    amount: i128,
    auto_approved: bool,
    is_processed: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Proposal {
    proposal_id: u64,
    proposer: String,
    proposal_type: String,
    new_value: i128,
    votes_for: i128,
    votes_against: i128,
    is_executed: bool,
}

// Demo implementation
struct InsurancePoolDemo {
    pool: InsurancePool,
    members: HashMap<String, PoolMember>,
    claims: HashMap<u64, Claim>,
    proposals: HashMap<u64, Proposal>,
    next_claim_id: u64,
    next_proposal_id: u64,
}

impl InsurancePoolDemo {
    fn new() -> Self {
        Self {
            pool: InsurancePool {
                total_funds: 0,
                total_members: 0,
                base_premium_rate_bps: 100, // 1%
                is_active: true,
            },
            members: HashMap::new(),
            claims: HashMap::new(),
            proposals: HashMap::new(),
            next_claim_id: 1,
            next_proposal_id: 1,
        }
    }

    fn join_pool(&mut self, user_id: String, premium: i128) -> Result<(), String> {
        if self.members.contains_key(&user_id) {
            return Err("User already in pool".to_string());
        }

        let risk_score = self.calculate_risk_score(&user_id);
        let member = PoolMember {
            user_id: user_id.clone(),
            premium_paid: premium,
            risk_score,
            claim_count: 0,
            is_active: true,
        };

        self.members.insert(user_id, member);
        self.pool.total_funds += premium;
        self.pool.total_members += 1;

        Ok(())
    }

    fn calculate_risk_score(&self, user_id: &str) -> u32 {
        // Simplified risk calculation
        // In real implementation, this would consider:
        // - Payment history
        // - Usage stability
        // - Device security
        // - Account tenure

        // Simple hash-based pseudo-random for demo
        let mut hash = 0u32;
        for byte in user_id.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }
        hash % 1000
    }

    fn submit_claim(&mut self, claimant: String, amount: i128) -> Result<u64, String> {
        if !self.members.contains_key(&claimant) {
            return Err("Not a pool member".to_string());
        }

        let member = self.members.get(&claimant).unwrap();
        if !member.is_active {
            return Err("Member not active".to_string());
        }

        let max_claim = self.pool.total_funds / 10; // 10% max
        if amount > max_claim {
            return Err("Claim amount too high".to_string());
        }

        let claim_id = self.next_claim_id;
        self.next_claim_id += 1;

        // Auto-approve small claims from low-risk members
        let auto_approve_threshold = self.pool.total_funds / 100; // 1%
        let auto_approved = amount <= auto_approve_threshold && member.risk_score <= 300;

        let claim = Claim {
            claim_id,
            claimant: claimant.clone(),
            amount,
            auto_approved,
            is_processed: false, // Start as false so process_claim can transition it
        };

        self.claims.insert(claim_id, claim);

        if auto_approved {
            self.process_claim(claim_id)?;
        }

        Ok(claim_id)
    }

    fn process_claim(&mut self, claim_id: u64) -> Result<(), String> {
        let claim = self.claims.get_mut(&claim_id).ok_or("Claim not found")?;

        if claim.is_processed {
            return Err("Claim already processed".to_string());
        }

        if self.pool.total_funds < claim.amount {
            return Err("Insufficient pool funds".to_string());
        }

        // Transfer funds (in real implementation, this would update meter balance)
        self.pool.total_funds -= claim.amount;
        claim.is_processed = true;

        // Update member claim count
        if let Some(member) = self.members.get_mut(&claim.claimant) {
            member.claim_count += 1;
        }

        Ok(())
    }

    fn create_proposal(
        &mut self,
        proposer: String,
        proposal_type: String,
        new_value: i128,
    ) -> Result<u64, String> {
        if !self.members.contains_key(&proposer) {
            return Err("Not a pool member".to_string());
        }

        let proposal_id = self.next_proposal_id;
        self.next_proposal_id += 1;

        let proposal = Proposal {
            proposal_id,
            proposer,
            proposal_type,
            new_value,
            votes_for: 0,
            votes_against: 0,
            is_executed: false,
        };

        self.proposals.insert(proposal_id, proposal);
        Ok(proposal_id)
    }

    fn vote_on_proposal(
        &mut self,
        voter: String,
        proposal_id: u64,
        vote_for: bool,
    ) -> Result<(), String> {
        if !self.members.contains_key(&voter) {
            return Err("Not a pool member".to_string());
        }

        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if proposal.is_executed {
            return Err("Proposal already executed".to_string());
        }

        let member = self.members.get(&voter).unwrap();
        let voting_power = member.premium_paid / 1_000_000; // 1 vote per XLM

        if vote_for {
            proposal.votes_for += voting_power;
        } else {
            proposal.votes_against += voting_power;
        }

        Ok(())
    }

    fn execute_proposal(&mut self, proposal_id: u64) -> Result<(), String> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if proposal.is_executed {
            return Err("Proposal already executed".to_string());
        }

        let total_votes = proposal.votes_for + proposal.votes_against;
        let approval_threshold = total_votes * 51 / 100; // 51% approval

        if proposal.votes_for < approval_threshold {
            return Err("Proposal rejected".to_string());
        }

        // Execute the proposal
        match proposal.proposal_type.as_str() {
            "ChangePremiumRate" => {
                self.pool.base_premium_rate_bps = proposal.new_value;
            }
            "EmergencyPause" => {
                self.pool.is_active = proposal.new_value > 0;
            }
            _ => return Err("Unknown proposal type".to_string()),
        }

        proposal.is_executed = true;
        Ok(())
    }

    fn get_pool_stats(&self) -> (i128, u32, i128) {
        (
            self.pool.total_funds,
            self.pool.total_members,
            self.pool.base_premium_rate_bps,
        )
    }
}

fn main() {
    println!("🏦 Stream Insurance Pool Governance Demo");
    println!("========================================\n");

    let mut demo = InsurancePoolDemo::new();

    // Demo scenario: Multiple users join the pool
    println!("📝 Step 1: Users join the insurance pool");

    let users = vec![
        ("alice", 5_000_000_000i128),   // 5000 XLM
        ("bob", 3_000_000_000i128),     // 3000 XLM
        ("charlie", 2_000_000_000i128), // 2000 XLM
    ];

    for (user, premium) in users {
        match demo.join_pool(user.to_string(), premium) {
            Ok(_) => println!("✅ {} joined with premium: {} stroops", user, premium),
            Err(e) => println!("❌ {} failed to join: {}", user, e),
        }
    }

    let (funds, members, rate) = demo.get_pool_stats();
    println!(
        "\n📊 Pool Stats: {} stroops, {} members, {}bps rate\n",
        funds, members, rate
    );

    // Demo scenario: Submit claims
    println!("🚨 Step 2: Submit insurance claims");

    // Small claim (auto-approved)
    match demo.submit_claim("alice".to_string(), 50_000_000) {
        // 50 XLM
        Ok(claim_id) => {
            let claim = demo.claims.get(&claim_id).unwrap();
            println!(
                "✅ Alice's claim #{}: {} stroops (auto-approved: {})",
                claim_id, claim.amount, claim.auto_approved
            );
        }
        Err(e) => println!("❌ Alice's claim failed: {}", e),
    }

    // Larger claim (requires approval)
    match demo.submit_claim("bob".to_string(), 500_000_000) {
        // 500 XLM
        Ok(claim_id) => {
            let claim = demo.claims.get(&claim_id).unwrap();
            println!(
                "✅ Bob's claim #{}: {} stroops (auto-approved: {})",
                claim_id, claim.amount, claim.auto_approved
            );
        }
        Err(e) => println!("❌ Bob's claim failed: {}", e),
    }

    let (funds, _, _) = demo.get_pool_stats();
    println!("\n📊 Pool funds after claims: {} stroops\n", funds);

    // Demo scenario: Governance proposal
    println!("🗳️  Step 3: Create and vote on governance proposal");

    match demo.create_proposal("alice".to_string(), "ChangePremiumRate".to_string(), 150) {
        Ok(proposal_id) => {
            println!(
                "✅ Alice created proposal #{}: Change premium rate to 150bps (1.5%)",
                proposal_id
            );

            // Members vote
            let _ = demo.vote_on_proposal("alice".to_string(), proposal_id, true);
            let _ = demo.vote_on_proposal("bob".to_string(), proposal_id, true);
            let _ = demo.vote_on_proposal("charlie".to_string(), proposal_id, false);

            let proposal = demo.proposals.get(&proposal_id).unwrap();
            println!(
                "📊 Votes: {} for, {} against",
                proposal.votes_for, proposal.votes_against
            );

            // Execute proposal
            match demo.execute_proposal(proposal_id) {
                Ok(_) => {
                    let (_, _, new_rate) = demo.get_pool_stats();
                    println!("✅ Proposal executed! New premium rate: {}bps", new_rate);
                }
                Err(e) => println!("❌ Proposal execution failed: {}", e),
            }
        }
        Err(e) => println!("❌ Proposal creation failed: {}", e),
    }

    println!("\n🎉 Demo completed successfully!");
    println!("\n💡 Key Features Demonstrated:");
    println!("   • Risk-based premium calculation");
    println!("   • Auto-approval for small, low-risk claims");
    println!("   • Democratic governance with voting");
    println!("   • Proposal execution with threshold checks");
    println!("   • Pool fund management and tracking");

    println!("\n🔗 Integration Points:");
    println!("   • Automatic fee allocation from utility claims");
    println!("   • Emergency funding for failing utility streams");
    println!("   • Priority access during network throttling");
    println!("   • Community-driven parameter adjustment");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_creation() {
        let demo = InsurancePoolDemo::new();
        assert_eq!(demo.pool.total_members, 0);
        assert_eq!(demo.pool.total_funds, 0);
        assert!(demo.pool.is_active);
    }

    #[test]
    fn test_join_pool() {
        let mut demo = InsurancePoolDemo::new();
        let result = demo.join_pool("test_user".to_string(), 1_000_000_000);
        assert!(result.is_ok());
        assert_eq!(demo.pool.total_members, 1);
        assert_eq!(demo.pool.total_funds, 1_000_000_000);
    }

    #[test]
    fn test_duplicate_join() {
        let mut demo = InsurancePoolDemo::new();
        demo.join_pool("test_user".to_string(), 1_000_000_000)
            .unwrap();
        let result = demo.join_pool("test_user".to_string(), 1_000_000_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_claim_submission() {
        let mut demo = InsurancePoolDemo::new();
        demo.join_pool("test_user".to_string(), 10_000_000_000)
            .unwrap(); // 10k XLM

        let result = demo.submit_claim("test_user".to_string(), 50_000_000); // 50 XLM
        assert!(result.is_ok());

        let claim_id = result.unwrap();
        let claim = demo.claims.get(&claim_id).unwrap();
        assert_eq!(claim.amount, 50_000_000);
    }

    #[test]
    fn test_governance_proposal() {
        let mut demo = InsurancePoolDemo::new();
        demo.join_pool("proposer".to_string(), 5_000_000_000)
            .unwrap();

        let result =
            demo.create_proposal("proposer".to_string(), "ChangePremiumRate".to_string(), 200);
        assert!(result.is_ok());

        let proposal_id = result.unwrap();
        let proposal = demo.proposals.get(&proposal_id).unwrap();
        assert_eq!(proposal.new_value, 200);
        assert!(!proposal.is_executed);
    }

    #[test]
    fn test_voting() {
        let mut demo = InsurancePoolDemo::new();
        demo.join_pool("voter".to_string(), 3_000_000_000).unwrap();

        let proposal_id = demo
            .create_proposal("voter".to_string(), "ChangePremiumRate".to_string(), 150)
            .unwrap();

        let result = demo.vote_on_proposal("voter".to_string(), proposal_id, true);
        assert!(result.is_ok());

        let proposal = demo.proposals.get(&proposal_id).unwrap();
        assert!(proposal.votes_for > 0);
    }
}
