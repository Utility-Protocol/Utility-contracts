//! Chainlink oracle integration (issue #130, step 2).
//!
//! Implements the Chainlink `AggregatorV3Interface` convention for Soroban:
//! a feed contract exposing `latest_round_data()` and `decimals()`. The adapter
//! reads the latest round, validates it, and normalizes it into an
//! [`OracleReport`].
//!
//! ## Response semantics
//!
//! Chainlink round data carries `answer` (the price, in the feed's `decimals`),
//! `started_at`, `updated_at`, and `answered_in_round`. A fresh, valid round
//! has a positive `answer` and a non-zero `updated_at`; both are enforced here
//! so a broken/misconfigured feed is surfaced as [`Error::InvalidValue`] /
//! [`Error::StaleFeed`] instead of silently poisoning the aggregate.

use soroban_sdk::{contractclient, contracttype, Address, Env, Symbol};

use crate::adapter::OracleAdapter;
use crate::types::{AdapterKind, Error, OracleReport};

/// Chainlink `AggregatorV3Interface` round data.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChainlinkRoundData {
    /// The round identifier (deprecated in Chainlink v3 but still returned).
    pub round_id: u64,
    /// The price/answer for this round, in the feed's decimals.
    pub answer: i128,
    /// When the round started (epoch-seconds).
    pub started_at: u64,
    /// When the round's answer was last updated (epoch-seconds).
    pub updated_at: u64,
    /// The round in which the answer was computed.
    pub answered_in_round: u64,
}

/// Cross-contract interface to a Chainlink-style feed.
#[contractclient(name = "ChainlinkFeedClient")]
pub trait ChainlinkFeed {
    /// Latest round data (Chainlink `AggregatorV3Interface::latestRoundData`).
    fn latest_round_data(env: Env) -> ChainlinkRoundData;
    /// Number of decimals the feed reports in (Chainlink `decimals`).
    fn decimals(env: Env) -> u32;
}

/// Adapter for Chainlink `AggregatorV3Interface` feeds.
pub struct ChainlinkAdapter;

impl OracleAdapter for ChainlinkAdapter {
    fn read(env: &Env, feed: &Address, data_key: &Symbol) -> Result<OracleReport, Error> {
        let client = ChainlinkFeedClient::new(env, feed);
        let round = client.latest_round_data();
        let decimals = client.decimals();

        if round.answer <= 0 {
            return Err(Error::InvalidValue);
        }
        if round.updated_at == 0 {
            return Err(Error::StaleFeed);
        }

        Ok(OracleReport {
            provider: feed.clone(),
            adapter: AdapterKind::Chainlink,
            data_key: data_key.clone(),
            value: round.answer,
            decimals,
            updated_at: round.updated_at,
        })
    }
}
