use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MixamoRig;

pub trait JointBuilder {
    fn build() -> Self;
}

pub struct Joint {
    name: String,
    joints: Vec<Joint>,
}

impl Joint {
    pub fn new(name: impl ToString) -> Self {
        let name = name.to_string();
        let joints = Default::default();
        Self { name, joints }
    }

    pub fn add(mut self, joint: Self) -> Self {
        self.joints.push(joint);
        self
    }

    pub fn child(&mut self, name: impl ToString) -> &mut Self {
        let joint = Joint::new(name);
        self.joints.push(joint);
        self.joints.last_mut().unwrap()
    }

    pub fn flatten(&self) -> Vec<crate::skeleton::Joint> {
        let mut joints = Vec::new();
        self.flatten_recursive(None, &mut joints);
        joints
    }

    fn flatten_recursive(
        &self,
        parent_index: Option<usize>,
        result: &mut Vec<crate::skeleton::Joint>,
    ) {
        let index = result.len();
        result.push(crate::skeleton::Joint::new(
            self.name.clone(),
            index,
            parent_index,
            fabelgeist_math::matrix::Mat4::identity(),
            Default::default(),
            Some(index),
        ));
        for child in &self.joints {
            child.flatten_recursive(Some(index), result);
        }
    }
}

macro_rules! joint {
    ($name:expr, ()) => {
        Joint::new($name)
    };
    ($name:expr, ($($child:tt),* $(,)?)) => {
        Joint::new($name)
            $(.add(joint! $child))*
    };
    (($name:expr, $children:tt)) => {
        joint!($name, $children)
    };
}

impl JointBuilder for Joint {
    fn build() -> Self {
        joint!(
            "mixamorig:Hips",
            (
                (
                    "mixamorig:LeftUpLeg",
                    ((
                        "mixamorig:LeftLeg",
                        ((
                            "mixamorig:LeftFoot",
                            (("mixamorig:LeftToeBase", (("mixamorig:LeftToe_End", ()))))
                        ))
                    ))
                ),
                (
                    "mixamorig:RightUpLeg",
                    ((
                        "mixamorig:RightLeg",
                        ((
                            "mixamorig:RightFoot",
                            (("mixamorig:RightToeBase", (("mixamorig:RightToe_End", ()))))
                        ))
                    ))
                ),
                (
                    "mixamorig:Spine",
                    ((
                        "mixamorig:Spine1",
                        ((
                            "mixamorig:Spine2",
                            (
                                (
                                    "mixamorig:Neck",
                                    (("mixamorig:Head", (("mixamorig:HeadTop_End", ()))))
                                ),
                                (
                                    "mixamorig:LeftShoulder",
                                    ((
                                        "mixamorig:LeftArm",
                                        ((
                                            "mixamorig:LeftForeArm",
                                            ((
                                                "mixamorig:LeftHand",
                                                (
                                                    (
                                                        "mixamorig:LeftHandThumb1",
                                                        ((
                                                            "mixamorig:LeftHandThumb2",
                                                            ((
                                                                "mixamorig:LeftHandThumb3",
                                                                (("mixamorig:LeftHandThumb4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:LeftHandIndex1",
                                                        ((
                                                            "mixamorig:LeftHandIndex2",
                                                            ((
                                                                "mixamorig:LeftHandIndex3",
                                                                (("mixamorig:LeftHandIndex4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:LeftHandMiddle1",
                                                        ((
                                                            "mixamorig:LeftHandMiddle2",
                                                            ((
                                                                "mixamorig:LeftHandMiddle3",
                                                                (("mixamorig:LeftHandMiddle4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:LeftHandRing1",
                                                        ((
                                                            "mixamorig:LeftHandRing2",
                                                            ((
                                                                "mixamorig:LeftHandRing3",
                                                                (("mixamorig:LeftHandRing4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:LeftHandPinky1",
                                                        ((
                                                            "mixamorig:LeftHandPinky2",
                                                            ((
                                                                "mixamorig:LeftHandPinky3",
                                                                (("mixamorig:LeftHandPinky4", ()))
                                                            ))
                                                        ))
                                                    )
                                                )
                                            ))
                                        ))
                                    ))
                                ),
                                (
                                    "mixamorig:RightShoulder",
                                    ((
                                        "mixamorig:RightArm",
                                        ((
                                            "mixamorig:RightForeArm",
                                            ((
                                                "mixamorig:RightHand",
                                                (
                                                    (
                                                        "mixamorig:RightHandThumb1",
                                                        ((
                                                            "mixamorig:RightHandThumb2",
                                                            ((
                                                                "mixamorig:RightHandThumb3",
                                                                (("mixamorig:RightHandThumb4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:RightHandIndex1",
                                                        ((
                                                            "mixamorig:RightHandIndex2",
                                                            ((
                                                                "mixamorig:RightHandIndex3",
                                                                (("mixamorig:RightHandIndex4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:RightHandMiddle1",
                                                        ((
                                                            "mixamorig:RightHandMiddle2",
                                                            ((
                                                                "mixamorig:RightHandMiddle3",
                                                                ((
                                                                    "mixamorig:RightHandMiddle4",
                                                                    ()
                                                                ))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:RightHandRing1",
                                                        ((
                                                            "mixamorig:RightHandRing2",
                                                            ((
                                                                "mixamorig:RightHandRing3",
                                                                (("mixamorig:RightHandRing4", ()))
                                                            ))
                                                        ))
                                                    ),
                                                    (
                                                        "mixamorig:RightHandPinky1",
                                                        ((
                                                            "mixamorig:RightHandPinky2",
                                                            ((
                                                                "mixamorig:RightHandPinky3",
                                                                (("mixamorig:RightHandPinky4", ()))
                                                            ))
                                                        ))
                                                    )
                                                )
                                            ))
                                        ))
                                    ))
                                )
                            )
                        ))
                    ))
                )
            )
        )
    }
}

impl MixamoRig {
    pub const JOINT_NAMES: &[&str] = &[
        "mixamorig:Hips",
        "mixamorig:Spine",
        "mixamorig:Spine1",
        "mixamorig:Spine2",
        "mixamorig:Neck",
        "mixamorig:Head",
        "mixamorig:LeftShoulder",
        "mixamorig:LeftArm",
        "mixamorig:LeftForeArm",
        "mixamorig:LeftHand",
        "mixamorig:RightShoulder",
        "mixamorig:RightArm",
        "mixamorig:RightForeArm",
        "mixamorig:RightHand",
        "mixamorig:LeftUpLeg",
        "mixamorig:LeftLeg",
        "mixamorig:LeftFoot",
        "mixamorig:LeftToeBase",
        "mixamorig:RightUpLeg",
        "mixamorig:RightLeg",
        "mixamorig:RightFoot",
        "mixamorig:RightToeBase",
        // Fingers - Left
        "mixamorig:LeftHandThumb1",
        "mixamorig:LeftHandThumb2",
        "mixamorig:LeftHandThumb3",
        "mixamorig:LeftHandIndex1",
        "mixamorig:LeftHandIndex2",
        "mixamorig:LeftHandIndex3",
        "mixamorig:LeftHandMiddle1",
        "mixamorig:LeftHandMiddle2",
        "mixamorig:LeftHandMiddle3",
        "mixamorig:LeftHandRing1",
        "mixamorig:LeftHandRing2",
        "mixamorig:LeftHandRing3",
        "mixamorig:LeftHandPinky1",
        "mixamorig:LeftHandPinky2",
        "mixamorig:LeftHandPinky3",
        // Fingers - Right
        "mixamorig:RightHandThumb1",
        "mixamorig:RightHandThumb2",
        "mixamorig:RightHandThumb3",
        "mixamorig:RightHandIndex1",
        "mixamorig:RightHandIndex2",
        "mixamorig:RightHandIndex3",
        "mixamorig:RightHandMiddle1",
        "mixamorig:RightHandMiddle2",
        "mixamorig:RightHandMiddle3",
        "mixamorig:RightHandRing1",
        "mixamorig:RightHandRing2",
        "mixamorig:RightHandRing3",
        "mixamorig:RightHandPinky1",
        "mixamorig:RightHandPinky2",
        "mixamorig:RightHandPinky3",
    ];

    pub fn is_mixamo_joint(name: &str) -> bool {
        name.starts_with("mixamorig:")
    }

    pub fn strip_prefix(name: &str) -> &str {
        name.strip_prefix("mixamorig:").unwrap_or(name)
    }

    pub fn skeleton() -> crate::skeleton::Skeleton {
        let root = Joint::build();
        crate::skeleton::Skeleton::new(root.flatten())
    }
}
