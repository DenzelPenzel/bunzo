pub mod bundle;
pub mod conflict;
pub mod proposer;
pub mod sender;
pub mod strategy;
pub mod task;
pub mod tracker;

pub use bundle::Bundle;
pub use conflict::ConflictDetector;
pub use proposer::{BundleProposer, BundleProposerImpl, ProposerError};
pub use strategy::{ConditionalSubmissionStrategy, DirectSubmissionStrategy, StrategyRouter};
pub use task::BundlerTask;
pub use tracker::{EscalationConfig, EscalationStrategy, TransactionTracker};

/// Bundle sender FSM states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleSenderState {
    /// Waiting for the next trigger to build a bundle
    Idle,
    /// Selecting operations and building a bundle
    Building,
    /// Submitting the signed transaction via strategy engine
    Submitting,
    /// Transaction submitted, waiting for it to appear in a block
    Pending,
    /// Fee bumping a stuck transaction
    Escalating,
    /// Cancelling a stale or unprofitable transaction
    Cancelling,
    /// Transaction mined, waiting for N confirmations
    Confirming,
    /// Terminal: bundle confirmed with sufficient depth
    Confirmed,
    /// Terminal: bundle was abandoned (e.g., became unprofitable)
    Abandoned,
}
