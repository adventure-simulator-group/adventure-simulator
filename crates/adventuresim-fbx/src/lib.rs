//! Reader for binary FBX files (Kaydara FBX Binary, versions 7100-7700).
//!
//! What is modelled: the node/property tree, the object table under `Objects`,
//! the connection list including the property names that object-property links
//! carry, and the animation curves reachable through them. Object resolution
//! follows OpenFBX (the loader momentum itself uses), because joint ordering in
//! a momentum character is defined by the order connections appear in the file.
//!
//! Deliberately free of heavy dependencies (`anyhow` and a pure-Rust inflate),
//! so an asset pipeline can read FBX without pulling in a tensor runtime.

pub mod curve;
pub mod link;
pub mod node;
pub mod node_animation;
pub mod object;
pub mod prop;
pub mod reader;
pub mod scene;
pub mod take;
pub mod transform_channel;

pub use curve::Curve;
pub use link::Link;
pub use node::Node;
pub use node_animation::NodeAnimation;
pub use object::Object;
pub use prop::Prop;
pub use reader::parse;
pub use scene::Scene;
pub use take::Take;
pub use transform_channel::TransformChannel;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_sampling_interpolation() {
        let curve = Curve {
            times: vec![0.0, 1.0, 2.0],
            values: vec![10.0, 20.0, 40.0],
        };
        assert_eq!(curve.sample(-0.5), Some(10.0));
        assert_eq!(curve.sample(0.0), Some(10.0));
        assert_eq!(curve.sample(0.5), Some(15.0));
        assert_eq!(curve.sample(1.0), Some(20.0));
        assert_eq!(curve.sample(1.5), Some(30.0));
        assert_eq!(curve.sample(2.5), Some(40.0));
    }

    #[test]
    fn prop_conversions() {
        let p_i32 = Prop::I32(42);
        assert_eq!(p_i32.as_i64(), Some(42));
        assert_eq!(p_i32.as_f64(), Some(42.0));

        let p_f32 = Prop::F32(3.14);
        assert!((p_f32.as_f64().unwrap() - 3.14).abs() < 1e-5);

        let p_arr = Prop::ArrF32(vec![1.0, 2.0, 3.0]);
        assert_eq!(p_arr.as_f64_array(), Some(vec![1.0, 2.0, 3.0]));
    }

    #[test]
    fn invalid_fbx_header_fails() {
        assert!(parse(b"not an fbx file").is_err());
    }
}

