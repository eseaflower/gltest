
#[allow(clippy::unreadable_literal)]
#[allow(clippy::unused_unit)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::manual_non_exhaustive)]
pub mod gl {
    pub use self::Gl as MyGl;
    include!("../../bindings/test_gl_bindings.rs");
}