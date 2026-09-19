//! Live renderer facade. Historical implementations and their tests are retained,
//! but desktop.rs/review.rs can only construct the optical liquid pipeline below.
#[path = "liquid_motion.rs"]
mod motion;

mod retained {
    include!("gpu.rs");
    pub mod optical {
        include!("liquid.rs");
    }
}
pub use retained::create_device;
pub use retained::optical::{Pipeline, Presenter, self_test};

pub use retained::optical::adaptive::{AdaptivePipeline, AdaptivePresenter, MaterialTuning};
pub use retained::optical::adaptive::self_test as adaptive_self_test;
