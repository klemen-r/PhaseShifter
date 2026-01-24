//! PhaseShifter Backtest Library
//!
//! Realistic backtesting for PhaseShifter zone interactions.
//! Streams through SCID tick data, generates clusters using phaseshifter-core,
//! tracks zone interactions with proper execution modeling.

pub mod bar_builder;
pub mod clusters;
pub mod feature_extractor;
pub mod mtf_tracker;
pub mod price_tracker;
pub mod scid;
pub mod session_tracker;
pub mod types;
pub mod volatility_tracker;
pub mod volume_tracker;
pub mod zone_tracker;

pub use clusters::{
    Cluster, ClusterConfig, ClusterManager, NodeStatus, ProjectionSide, TrackedNode,
};
pub use feature_extractor::{FeatureExtractor, FeatureSnapshot};
pub use mtf_tracker::{MtfTracker, Phase};
pub use price_tracker::PriceTracker;
pub use session_tracker::SessionTracker;
pub use types::*;
pub use volatility_tracker::VolatilityTracker;
pub use volume_tracker::VolumeTracker;
pub use zone_tracker::ZoneTracker;
