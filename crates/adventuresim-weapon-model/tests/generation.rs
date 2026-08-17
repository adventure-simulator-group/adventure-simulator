use std::collections::HashMap;

use adventuresim_weapon_model::{
    Attachment, CodecError, ComponentDesign, ComponentRole, ComponentShape, CylinderSpec,
    MELEE_CATALOG_IDS, MaceSpec, MaterialClass, Millimeters, OffsetMm, PRESET_IDS, Permille,
    Segments, ValidationError, WeaponDesign, WeaponHolderKind, WeaponIconLayout, WeaponIconSpec,
    decode, decode_holder, default_design, default_holder_design, derive_properties, design_hash,
    encode, encode_holder, generate, generate_holder, generate_holder_icon, generate_icon,
    holder_design_hash, icon_layout, preset_design, recommended_holder, validate, validate_holder,
};

fn signed_volume(positions: &[[f32; 3]], indices: &[u32]) -> f32 {
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| {
            let [a, b, c] = [
                positions[triangle[0] as usize],
                positions[triangle[1] as usize],
                positions[triangle[2] as usize],
            ];
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

fn assert_closed_and_outward(design: &WeaponDesign) {
    let generated = generate(design).expect("generated weapon");
    assert_parts_closed_and_outward(&generated.parts);
}

fn assert_parts_closed_and_outward(parts: &[adventuresim_weapon_model::MeshPart]) {
    for part in parts {
        assert_eq!(
            part.positions.len(),
            part.normals.len(),
            "{}",
            part.component_id
        );
        assert_eq!(part.indices.len() % 3, 0);
        assert!(
            signed_volume(&part.positions, &part.indices) > 1e-10,
            "{} winding",
            part.component_id
        );
        assert!(
            part.positions
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        assert!(part.normals.iter().flatten().all(|value| value.is_finite()));
        assert!(part.normals.iter().all(|normal| {
            ((normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt() - 1.0)
                .abs()
                < 1e-4
        }));
        let vertex_key = |index: u32| part.positions[index as usize].map(f32::to_bits);
        let mut edges = HashMap::<([u32; 3], [u32; 3]), u8>::new();
        for triangle in part.indices.as_chunks::<3>().0 {
            for [a, b] in [
                [triangle[0], triangle[1]],
                [triangle[1], triangle[2]],
                [triangle[2], triangle[0]],
            ] {
                let a = vertex_key(a);
                let b = vertex_key(b);
                *edges
                    .entry(if a < b { (a, b) } else { (b, a) })
                    .or_default() += 1;
            }
        }
        assert!(
            edges.values().all(|incidence| *incidence == 2),
            "{} is not a closed indexed solid",
            part.component_id
        );
        for triangle in part.indices.as_chunks::<3>().0 {
            let [a, b, c] = [
                part.positions[triangle[0] as usize],
                part.positions[triangle[1] as usize],
                part.positions[triangle[2] as usize],
            ];
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let face = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            assert!(
                face[0] * face[0] + face[1] * face[1] + face[2] * face[2] > 1e-16,
                "{} degenerate face",
                part.component_id
            );
        }
    }
}

#[test]
fn every_non_polearm_has_a_fitted_closed_holder() {
    let mut blade_sheaths = 0;
    let mut haft_loops = 0;
    let mut polearms = 0;
    for id in MELEE_CATALOG_IDS {
        let design = default_design(id).unwrap();
        let generated = default_holder_design(&design);
        assert_eq!(
            generated.as_ref().map(|holder| holder.kind),
            recommended_holder(id)
        );
        match generated {
            Some(holder_design) => {
                let holder = generate_holder(&holder_design).unwrap();
                assert_eq!(holder.design_hash, holder_design_hash(&holder_design));
                assert!(!holder.parts.is_empty(), "{id}");
                assert_parts_closed_and_outward(&holder.parts);
                assert!(holder.bounds.max[1] > holder.bounds.min[1], "{id}");
                match holder.kind {
                    WeaponHolderKind::BladeSheath => blade_sheaths += 1,
                    WeaponHolderKind::HaftLoop => haft_loops += 1,
                }
            }
            None => polearms += 1,
        }
    }
    assert_eq!((blade_sheaths, haft_loops, polearms), (14, 4, 5));
}

#[test]
fn sheath_tracks_blade_dimensions_and_leaves_the_hilt_exposed() {
    let design = default_design("longsword").unwrap();
    let weapon = generate(&design).unwrap();
    let holder = generate_holder(&default_holder_design(&design).unwrap()).unwrap();
    assert_eq!(holder.kind, WeaponHolderKind::BladeSheath);
    let blade = weapon
        .parts
        .iter()
        .find(|part| part.component_id == "blade")
        .unwrap();
    assert!(holder.bounds.min[0] < blade.bounds.min[0]);
    assert!(holder.bounds.max[0] > blade.bounds.max[0]);
    assert!(holder.bounds.min[2] < blade.bounds.min[2]);
    assert!(holder.bounds.max[2] > blade.bounds.max[2]);
    assert!(holder.bounds.min[1] > holder.grip[1]);

    let mut longer = design.clone();
    let blade = longer
        .components
        .iter_mut()
        .find(|part| part.id == "blade")
        .unwrap();
    match &mut blade.shape {
        ComponentShape::SectionBlade(spec) => spec.length = Millimeters(spec.length.0 + 120),
        _ => panic!("longsword blade changed shape"),
    }
    let longer_holder = generate_holder(&default_holder_design(&longer).unwrap()).unwrap();
    assert!(longer_holder.bounds.max[1] > holder.bounds.max[1] + 0.10);
}

#[test]
fn haft_loop_tracks_the_grip_instead_of_the_head() {
    let design = default_design("flanged_mace").unwrap();
    let holder = generate_holder(&default_holder_design(&design).unwrap()).unwrap();
    assert_eq!(holder.kind, WeaponHolderKind::HaftLoop);
    assert!(holder.bounds.min[1] < holder.grip[1]);
    assert!(holder.bounds.max[1] > holder.grip[1]);

    let mut wider = design.clone();
    let grip = wider
        .components
        .iter_mut()
        .find(|part| part.role == ComponentRole::Grip)
        .unwrap();
    match &mut grip.shape {
        ComponentShape::Cylinder(spec) => spec.radius = Millimeters(spec.radius.0 + 4),
        _ => panic!("mace grip changed shape"),
    }
    let wider_holder = generate_holder(&default_holder_design(&wider).unwrap()).unwrap();
    assert!(wider_holder.bounds.max[0] > holder.bounds.max[0] + 0.003);
}

#[test]
fn holder_templates_encode_independent_per_instance_parameters() {
    let weapon = default_design("longsword").unwrap();
    let first = default_holder_design(&weapon).unwrap();
    validate_holder(&first).unwrap();
    let bytes = encode_holder(&first).unwrap();
    assert_eq!(decode_holder(&bytes).unwrap(), first);

    let mut second = first.clone();
    second.clearance = Millimeters(first.clearance.0 + 3);
    second.chape_length = Millimeters(first.chape_length.0 + 8);
    assert_ne!(holder_design_hash(&first), holder_design_hash(&second));
    assert_ne!(
        encode_holder(&first).unwrap(),
        encode_holder(&second).unwrap()
    );
    let first_mesh = generate_holder(&first).unwrap();
    let second_mesh = generate_holder(&second).unwrap();
    assert!(second_mesh.bounds.max[0] > first_mesh.bounds.max[0]);

    let mut invalid = first;
    invalid.clearance = Millimeters(0);
    assert!(validate_holder(&invalid).is_err());
    assert!(encode_holder(&invalid).is_err());
    assert!(default_holder_design(&default_design("halberd").unwrap()).is_none());

    let mut hostile = default_holder_design(&weapon).unwrap();
    hostile.hanger_height = Millimeters(u32::MAX);
    let result = std::panic::catch_unwind(|| generate_holder(&hostile));
    assert!(matches!(result, Ok(Err(_))));
}

#[test]
fn holder_parameter_extremes_stay_valid_closed_and_distinct() {
    for id in MELEE_CATALOG_IDS {
        let weapon = default_design(id).unwrap();
        let Some(default) = default_holder_design(&weapon) else {
            continue;
        };
        let mut minimum = default.clone();
        minimum.clearance = Millimeters(2);
        minimum.throat_length = Millimeters(4);
        minimum.chape_length = Millimeters(6);
        minimum.loop_position = Permille(0);
        minimum.loop_bar_radius = Millimeters(2);
        minimum.hanger_width = Millimeters(20);
        minimum.hanger_height = Millimeters(30);
        let mut maximum = default;
        maximum.clearance = Millimeters(20);
        maximum.throat_length = Millimeters(40);
        maximum.chape_length = Millimeters(60);
        maximum.loop_position = Permille(1_000);
        maximum.loop_bar_radius = Millimeters(12);
        maximum.hanger_width = Millimeters(120);
        maximum.hanger_height = Millimeters(180);
        for design in [&minimum, &maximum] {
            validate_holder(design).unwrap_or_else(|errors| panic!("{id}: {errors:?}"));
            let holder = generate_holder(design).unwrap();
            assert_parts_closed_and_outward(&holder.parts);
            assert!(holder.derived.mass_kg.is_finite() && holder.derived.mass_kg > 0.0);
            assert!(holder.derived.length_m.is_finite() && holder.derived.length_m > 0.0);
        }
        assert_ne!(
            holder_design_hash(&minimum),
            holder_design_hash(&maximum),
            "{id}"
        );
    }
}

#[test]
fn every_existing_melee_catalog_id_has_a_valid_deterministic_solid() {
    assert_eq!(MELEE_CATALOG_IDS.len(), 23);
    for id in MELEE_CATALOG_IDS {
        let design = default_design(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(design.catalog_id, *id);
        validate(&design).unwrap_or_else(|errors| panic!("{id}: {errors:?}"));
        assert_closed_and_outward(&design);
        let generated = generate(&design).unwrap();
        assert!(generated.derived.mass_kg > 0.0, "{id}");
        assert!(generated.derived.length_m > 0.0, "{id}");
        assert!(generated.derived.grip_to_tip_m > 0.0, "{id}");
        assert!(
            generated
                .anchors
                .iter()
                .any(|anchor| anchor.name == "weapon.grip")
        );
        assert!(
            generated
                .anchors
                .iter()
                .any(|anchor| anchor.name == "weapon.tip")
        );
        assert_eq!(generated.design_hash, design_hash(&design));
    }
    assert!(default_design("bow").is_none());
}

#[test]
fn postcard_transport_and_design_hash_are_canonical_and_sensitive() {
    let design = default_design("longsword").unwrap();
    let first = encode(&design).unwrap();
    let second = encode(&design).unwrap();
    assert_eq!(first, second);
    assert_eq!(decode(&first).unwrap(), design);
    assert_eq!(design_hash(&design), design_hash(&decode(&first).unwrap()));
    let mut changed = design.clone();
    match &mut changed.components.last_mut().unwrap().shape {
        ComponentShape::Blade(blade) => blade.length = Millimeters(blade.length.0 + 1),
        ComponentShape::SectionBlade(blade) => blade.length = Millimeters(blade.length.0 + 1),
        _ => panic!("expected blade"),
    }
    assert_ne!(design_hash(&design), design_hash(&changed));
    assert_ne!(encode(&design).unwrap(), encode(&changed).unwrap());
}

#[test]
fn unusual_polearm_flanged_mace_uses_the_same_attachment_graph() {
    let design = WeaponDesign {
        catalog_id: "developer_pole_mace".into(),
        components: vec![
            ComponentDesign {
                id: "shaft".into(),
                role: ComponentRole::Grip,
                attachment: Attachment::Root,
                offset: OffsetMm::default(),
                material: MaterialClass::Wood,
                shape: ComponentShape::Cylinder(CylinderSpec {
                    length: Millimeters(1_850),
                    radius: Millimeters(25),
                    bottom_scale: Permille(1000),
                    top_scale: Permille(1000),
                    segments: Segments(16),
                }),
            },
            ComponentDesign {
                id: "socket".into(),
                role: ComponentRole::Socket,
                attachment: Attachment::TopOf {
                    component: "shaft".into(),
                    insertion: Millimeters(180),
                },
                offset: OffsetMm::default(),
                material: MaterialClass::Steel,
                shape: ComponentShape::Cylinder(CylinderSpec {
                    length: Millimeters(240),
                    radius: Millimeters(31),
                    bottom_scale: Permille(1000),
                    top_scale: Permille(1000),
                    segments: Segments(18),
                }),
            },
            ComponentDesign {
                id: "mace_head".into(),
                role: ComponentRole::Head,
                attachment: Attachment::TopOf {
                    component: "socket".into(),
                    insertion: Millimeters(20),
                },
                offset: OffsetMm::default(),
                material: MaterialClass::Steel,
                shape: ComponentShape::Mace(MaceSpec {
                    length: Millimeters(210),
                    core_radius: Millimeters(16),
                    cusp_radius: Millimeters(72),
                    flanges: 8,
                    flange_thickness: Millimeters(5),
                    segments: Segments(18),
                    cusp_height: Permille(620),
                }),
            },
        ],
    };
    validate(&design).unwrap();
    assert_closed_and_outward(&design);
    let generated = generate(&design).unwrap();
    assert_eq!(generated.parts.len(), 3);
    assert!(generated.derived.length_m > 2.0);
    let socket_top = generated
        .anchors
        .iter()
        .find(|anchor| anchor.name == "socket.top")
        .unwrap();
    let head_base = generated
        .anchors
        .iter()
        .find(|anchor| anchor.name == "mace_head.base")
        .unwrap();
    assert!((socket_top.position[1] - head_base.position[1] - 0.02).abs() < 1e-6);
}

#[test]
fn validation_and_transport_reject_invalid_attachment_graphs() {
    let mut missing = default_design("longsword").unwrap();
    missing.components[2].attachment = Attachment::TopOf {
        component: "missing".into(),
        insertion: Millimeters(0),
    };
    assert!(matches!(
        validate(&missing),
        Err(errors) if errors.iter().any(|error| matches!(error, ValidationError::MissingParent { .. }))
    ));
    assert!(matches!(
        encode(&missing),
        Err(CodecError::InvalidDesign(_))
    ));

    let mut cycle = default_design("flanged_mace").unwrap();
    cycle.components[0].attachment = Attachment::TopOf {
        component: "head".into(),
        insertion: Millimeters(0),
    };
    assert!(matches!(
        validate(&cycle),
        Err(errors) if errors.iter().any(|error| matches!(error, ValidationError::AttachmentCycle(_)))
    ));
    assert!(decode(&[0xff, 0x00]).is_err());

    let mut oversized = default_design("flanged_mace").unwrap();
    match &mut oversized.components.last_mut().unwrap().shape {
        ComponentShape::Mace(head) => head.segments = Segments(u16::MAX),
        ComponentShape::GothicMace(head) => head.radial_segments = Segments(u16::MAX),
        _ => panic!("mace head"),
    }
    assert!(matches!(
        validate(&oversized),
        Err(errors) if errors.iter().any(|error| matches!(error, ValidationError::InvalidDimensions(_)))
    ));

    for attack in [
        {
            let mut value = default_design("longsword").unwrap();
            value.components[2].attachment = Attachment::TopOf {
                component: "guard".into(),
                insertion: Millimeters(u32::MAX),
            };
            value
        },
        {
            let mut value = default_design("longsword").unwrap();
            value.components[2].offset.x = 6_000;
            value
        },
        {
            let mut value = default_design("longsword").unwrap();
            value.components[2].attachment = Attachment::Root;
            value
        },
    ] {
        assert!(validate(&attack).is_err());
        assert!(encode(&attack).is_err());
    }

    let mut overlong = WeaponDesign {
        catalog_id: "overlong-chain".into(),
        components: Vec::new(),
    };
    for index in 0..3 {
        overlong.components.push(ComponentDesign {
            id: format!("section-{index}"),
            role: if index == 0 {
                ComponentRole::Grip
            } else {
                ComponentRole::Structure
            },
            attachment: if index == 0 {
                Attachment::Root
            } else {
                Attachment::TopOf {
                    component: format!("section-{}", index - 1),
                    insertion: Millimeters(0),
                }
            },
            offset: OffsetMm::default(),
            material: MaterialClass::Wood,
            shape: ComponentShape::Cylinder(CylinderSpec {
                length: Millimeters(5_000),
                radius: Millimeters(20),
                bottom_scale: Permille(1000),
                top_scale: Permille(1000),
                segments: Segments(12),
            }),
        });
    }
    assert!(matches!(
        validate(&overlong),
        Err(errors) if errors.contains(&ValidationError::OverallBoundsExceeded)
    ));
}

#[test]
fn deterministic_parameter_fuzz_stays_finite_closed_and_outward() {
    let mut state = 0x1544_9e37_u64;
    let mut next = |limit: u32| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 32) as u32) % limit
    };
    for id in PRESET_IDS {
        for _ in 0..8 {
            let mut design = preset_design(id).unwrap();
            for component in &mut design.components {
                match &mut component.shape {
                    ComponentShape::Cylinder(value) => {
                        value.radius.0 += next(3);
                    }
                    ComponentShape::Blade(value) => {
                        value.curvature.0 += next(5) as i32 - 2;
                    }
                    ComponentShape::Guard(value) => {
                        value.sweep.0 += next(5) as i32 - 2;
                    }
                    ComponentShape::Mace(value) => {
                        value.cusp_height.0 += next(3) as u16;
                    }
                    ComponentShape::Socket(v) => v.wall.0 += next(2),
                    ComponentShape::Langet(v) => v.thickness.0 += next(2),
                    ComponentShape::SectionBlade(v) => v.curvature.0 += next(3) as i32,
                    ComponentShape::Axe(v) => v.curvature.0 += next(3) as u16,
                    ComponentShape::HammerPoll(v) => v.crown.0 += next(3) as u16,
                    ComponentShape::CurvedBeak(v) => v.curvature.0 += next(3) as i32,
                    ComponentShape::FacetedBeak(v) => v.set.0 += next(3) as i32,
                    ComponentShape::Glaive(v) => v.curvature.0 += next(3) as i32,
                    ComponentShape::Bill(v) => v.hook_curvature.0 += next(3) as u16,
                    ComponentShape::Fork(v) => v.taper.0 += next(3) as u16,
                    ComponentShape::Partisan(v) => v.belly.0 += next(3) as u16,
                    ComponentShape::TubePath(v) => v.radius.0 += next(2),
                    ComponentShape::RingGuard(v) => v.radius.0 += next(2),
                    ComponentShape::FigureEight(v) => v.width.0 += next(2),
                    ComponentShape::FanPommel(v) => v.width.0 += next(2),
                    ComponentShape::Rondel(v) => v.radius.0 += next(2),
                    ComponentShape::GothicMace(v) => v.concavity.0 += next(3) as u16,
                    ComponentShape::SlabGrip(v) => v.scale_thickness.0 += next(2),
                    ComponentShape::KnuckleBow(v) => v.bulge.0 += next(3) as u16,
                    ComponentShape::Collar(v) => v.radius.0 += next(2),
                    ComponentShape::Sleeve(v) => v.wall.0 += next(2),
                    ComponentShape::Boss(v) => v.radius.0 += next(2),
                    ComponentShape::Spear(v) => v.acuteness.0 += next(3) as u16,
                    ComponentShape::ProfiledPommel(v) => v.profile[0].radius.0 += next(2),
                }
            }
            validate(&design).unwrap_or_else(|errors| panic!("{id}: {errors:?}"));
            assert_closed_and_outward(&design);
            generate_icon(
                &design,
                WeaponIconSpec {
                    size: 32,
                    supersampling: 2,
                },
            )
            .unwrap_or_else(|error| panic!("{id} icon: {error}"));
        }
    }
}

#[test]
fn procedural_icons_obey_focus_orientation_and_clipping_contracts() {
    for id in MELEE_CATALOG_IDS {
        let design = default_design(id).unwrap();
        let icon = generate_icon(
            &design,
            WeaponIconSpec {
                size: 96,
                supersampling: 4,
            },
        )
        .unwrap_or_else(|error| panic!("{id}: {error}"));
        let expected = icon_layout(&design);
        let expected_from_catalog_family = if matches!(
            *id,
            "arming_sword"
                | "baselard"
                | "bauernwehr"
                | "falchion"
                | "katzbalger"
                | "knife"
                | "kriegsmesser"
                | "longsword"
                | "messer"
                | "misericorde"
                | "rapier"
                | "rondel_dagger"
                | "utility_knife"
                | "zweihander"
        ) {
            WeaponIconLayout::HiltFocus
        } else {
            WeaponIconLayout::HeadFocus
        };
        assert_eq!(expected, expected_from_catalog_family, "{id} family");
        assert_eq!(icon.layout, expected, "{id}");
        assert_eq!(icon.framing_anchor, [0.5, 0.5], "{id} semantic anchor");
        assert!(
            (1.0..=2.0).contains(&icon.head_zoom),
            "{id} invalid head zoom {}",
            icon.head_zoom
        );
        assert_eq!(icon.alpha.len(), 96 * 96, "{id}");
        let occupied = icon.alpha.iter().filter(|value| **value > 0).count();
        assert!(occupied > 96, "{id} icon is effectively empty");
        assert!(
            occupied < 96 * 96 / 2,
            "{id} icon consumes the whole square"
        );
        match expected {
            WeaponIconLayout::HiltFocus => {
                assert!(icon.focus_bounds.min[0] >= 0.01, "{id} hilt clipped left");
                assert!(icon.focus_bounds.min[1] >= 0.01, "{id} hilt clipped top");
                assert!(icon.focus_bounds.max[0] <= 0.99, "{id} hilt clipped right");
                assert!(icon.focus_bounds.max[1] <= 0.99, "{id} hilt clipped bottom");
                assert!(
                    icon.occupied_bounds.max[1] >= 0.97,
                    "{id} blade did not clip bottom"
                );
                assert!(
                    icon.occupied_bounds.min[0] < 0.25,
                    "{id} blade does not extend toward lower-left"
                );
            }
            WeaponIconLayout::HeadFocus => {
                assert!(icon.focus_bounds.min[0] >= 0.01, "{id} head clipped left");
                assert!(icon.focus_bounds.min[1] >= 0.01, "{id} head clipped top");
                assert!(icon.focus_bounds.max[0] <= 0.99, "{id} head clipped right");
                assert!(icon.focus_bounds.max[1] <= 0.99, "{id} head clipped bottom");
                assert!(
                    icon.occupied_bounds.max[1] >= 0.97,
                    "{id} shaft did not clip bottom"
                );
                assert!(
                    icon.occupied_bounds.max[0] >= 0.97,
                    "{id} shaft does not extend toward lower-right"
                );
                if icon.head_zoom < 1.99 {
                    assert!(
                        icon.focus_bounds.min[0] <= 0.15 || icon.focus_bounds.min[1] <= 0.15,
                        "{id} head did not approach the inset corner"
                    );
                }
                if matches!(*id, "war_hammer" | "walking_staff") {
                    assert!(
                        icon.head_zoom > 1.5,
                        "{id} exception did not materially enlarge the head"
                    );
                }
            }
        }
        let repeated = generate_icon(
            &design,
            WeaponIconSpec {
                size: 96,
                supersampling: 4,
            },
        )
        .unwrap();
        assert_eq!(icon.alpha, repeated.alpha, "{id} icon is not deterministic");
        let png = icon.encode_png().unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "{id}");
        assert_eq!(png[25], 6, "{id}: CSS mask PNG must carry RGBA alpha");
    }
}

#[test]
fn procedural_holder_icons_are_fitted_mirrored_and_deterministic() {
    let spec = WeaponIconSpec {
        size: 96,
        supersampling: 4,
    };
    let sword = default_design("longsword").unwrap();
    let sheath = default_holder_design(&sword).unwrap();
    let icon = generate_holder_icon(&sheath, spec).unwrap();
    assert_eq!(icon.layout, WeaponIconLayout::HiltFocus);
    assert!(icon.mirrored);
    assert_eq!(icon.framing_anchor, [0.24, 0.24]);
    assert!(icon.focus_bounds.min[0] >= 0.01);
    assert!(icon.focus_bounds.min[1] >= 0.01);
    assert!(icon.focus_bounds.max[0] <= 0.99);
    assert!(icon.focus_bounds.max[1] <= 0.99);
    assert!(
        icon.occupied_bounds.max[0] >= 0.97 && icon.occupied_bounds.max[1] >= 0.97,
        "scabbard body must exit toward the lower-right: {:?}",
        icon.occupied_bounds
    );
    assert_eq!(
        icon.alpha,
        generate_holder_icon(&sheath, spec).unwrap().alpha,
        "holder icon is not deterministic"
    );

    let hammer = default_design("war_hammer").unwrap();
    let haft_loop = default_holder_design(&hammer).unwrap();
    let loop_icon = generate_holder_icon(&haft_loop, spec).unwrap();
    assert_eq!(loop_icon.layout, WeaponIconLayout::HeadFocus);
    assert!(!loop_icon.mirrored);
    assert_eq!(loop_icon.framing_anchor, [0.5, 0.5]);
    assert!(loop_icon.occupied_bounds.min[0] >= 0.01);
    assert!(loop_icon.occupied_bounds.min[1] >= 0.01);
    assert!(loop_icon.occupied_bounds.max[0] <= 0.99);
    assert!(loop_icon.occupied_bounds.max[1] <= 0.99);
    assert_ne!(icon.alpha, loop_icon.alpha);

    for id in MELEE_CATALOG_IDS {
        let weapon = default_design(id).unwrap();
        let Some(holder) = default_holder_design(&weapon) else {
            continue;
        };
        generate_holder_icon(
            &holder,
            WeaponIconSpec {
                size: 32,
                supersampling: 2,
            },
        )
        .unwrap_or_else(|error| panic!("{id} holder icon: {error}"));
    }
}

#[test]
fn accepted_modeler_presets_cover_the_high_fidelity_vocabulary() {
    assert_eq!(PRESET_IDS.len(), 21);
    let mut vocabulary = std::collections::HashSet::new();
    for id in PRESET_IDS {
        let design = preset_design(id).unwrap_or_else(|| panic!("missing preset {id}"));
        validate(&design).unwrap_or_else(|errors| panic!("{id}: {errors:?}"));
        assert_closed_and_outward(&design);
        for component in &design.components {
            let name = match component.shape {
                ComponentShape::Socket(_) => "socket",
                ComponentShape::Langet(_) => "langet",
                ComponentShape::SectionBlade(_) => "section-blade",
                ComponentShape::Axe(_) => "axe",
                ComponentShape::HammerPoll(_) => "hammer-poll",
                ComponentShape::CurvedBeak(_) => "curved-beak",
                ComponentShape::FacetedBeak(_) => "faceted-beak",
                ComponentShape::Glaive(_) => "glaive",
                ComponentShape::Bill(_) => "bill",
                ComponentShape::Fork(_) => "fork",
                ComponentShape::Partisan(_) => "partisan",
                ComponentShape::TubePath(_) => "tube-path",
                ComponentShape::RingGuard(_) => "ring",
                ComponentShape::FigureEight(_) => "figure-eight",
                ComponentShape::FanPommel(_) => "fan-pommel",
                ComponentShape::Rondel(_) => "rondel",
                ComponentShape::GothicMace(_) => "gothic-mace",
                ComponentShape::SlabGrip(_) => "slab-grip",
                ComponentShape::KnuckleBow(_) => "knuckle-bow",
                ComponentShape::Collar(_) => "collar",
                ComponentShape::Sleeve(_) => "sleeve",
                ComponentShape::Boss(_) => "boss",
                ComponentShape::Spear(_) => "spear",
                ComponentShape::ProfiledPommel(_) => "profiled-pommel",
                ComponentShape::Cylinder(_)
                | ComponentShape::Blade(_)
                | ComponentShape::Guard(_)
                | ComponentShape::Mace(_) => continue,
            };
            vocabulary.insert(name);
        }
    }
    for required in [
        "socket",
        "langet",
        "section-blade",
        "axe",
        "hammer-poll",
        "curved-beak",
        "faceted-beak",
        "glaive",
        "bill",
        "fork",
        "partisan",
        "tube-path",
        "ring",
        "figure-eight",
        "fan-pommel",
        "rondel",
        "gothic-mace",
    ] {
        assert!(vocabulary.contains(required), "missing {required}");
    }
}

#[test]
fn recipe_derivation_is_deterministic_and_mesh_independent() {
    for id in PRESET_IDS {
        let design = preset_design(id).unwrap();
        let first = derive_properties(&design).unwrap();
        assert_eq!(first, derive_properties(&design).unwrap(), "{id}");
        assert!(first.mass_kg > 0.0, "{id}");
        assert!(first.length_m > 0.0, "{id}");
        assert!(first.grip_to_tip_m > 0.0, "{id}");
    }
}

#[test]
fn curved_bill_and_gothic_flange_profiles_are_dense_and_parameter_sensitive() {
    let bill = preset_design("hooked-bill").unwrap();
    let generated = generate(&bill).unwrap();
    let hook = generated
        .parts
        .iter()
        .find(|part| part.component_id == "bill")
        .unwrap();
    assert!(
        hook.positions.len() >= 60,
        "continuous hook sampling regressed"
    );
    assert!(hook.bounds.max[0] - hook.bounds.min[0] > 0.18);
    let mut changed = bill.clone();
    let ComponentShape::Bill(spec) = &mut changed
        .components
        .iter_mut()
        .find(|component| component.id == "bill")
        .unwrap()
        .shape
    else {
        panic!("bill")
    };
    spec.hook_curvature.0 += 90;
    assert_ne!(
        generate(&bill).unwrap().parts,
        generate(&changed).unwrap().parts
    );

    let mace = preset_design("gothic-flanged-mace").unwrap();
    let mut changed = mace.clone();
    let ComponentShape::GothicMace(spec) = &mut changed.components.last_mut().unwrap().shape else {
        panic!("gothic mace")
    };
    spec.concavity.0 -= 150;
    assert_ne!(
        generate(&mace).unwrap().parts,
        generate(&changed).unwrap().parts
    );
}

fn structural_kind(component: &ComponentDesign) -> &'static str {
    if component.id == "shaft" {
        return "shaft";
    }
    if component.id == "butt-cap" {
        return "pommel";
    }
    match &component.shape {
        ComponentShape::Cylinder(_) if component.role == ComponentRole::Grip => "grip",
        ComponentShape::Cylinder(_) => "cylinder",
        ComponentShape::Socket(_) => "socket",
        ComponentShape::Langet(_) => "box",
        ComponentShape::SectionBlade(_) => "sectionBlade",
        ComponentShape::Blade(_) => "blade",
        ComponentShape::Spear(_) => "spear",
        ComponentShape::Axe(_) => "axe",
        ComponentShape::HammerPoll(_) => "hammer",
        ComponentShape::CurvedBeak(_) => "beak",
        ComponentShape::FacetedBeak(_) => "facetedBeak",
        ComponentShape::Glaive(_) => "glaive",
        ComponentShape::Bill(_) => "bill",
        ComponentShape::Fork(_) => "fork",
        ComponentShape::Partisan(_) => "partisan",
        ComponentShape::Guard(_) => "guard",
        ComponentShape::TubePath(_) => "tube",
        ComponentShape::RingGuard(_) => "ringGuard",
        ComponentShape::FigureEight(_) => "figureEight",
        ComponentShape::FanPommel(_) => "fanPommel",
        ComponentShape::Rondel(_) | ComponentShape::ProfiledPommel(_) | ComponentShape::Boss(_) => {
            "pommel"
        }
        ComponentShape::GothicMace(_) | ComponentShape::Mace(_) => "mace",
        ComponentShape::SlabGrip(_) => "slabGrip",
        ComponentShape::KnuckleBow(_) => "knuckleBow",
        ComponentShape::Collar(_) => "collar",
        ComponentShape::Sleeve(_) => "sleeve",
    }
}

#[test]
fn all_accepted_presets_match_the_js_structural_fixture_and_key_controls() {
    let fixture: &[(&str, &[&str])] = &[
        (
            "halberd-1540",
            &[
                "shaft", "socket", "pommel", "axe", "beak", "spear", "box", "box",
            ],
        ),
        (
            "lucerne-hammer",
            &[
                "shaft", "socket", "pommel", "hammer", "beak", "spear", "box", "box",
            ],
        ),
        (
            "pollaxe",
            &[
                "shaft", "socket", "pommel", "axe", "hammer", "spear", "box", "box",
            ],
        ),
        ("kriegsspiess", &["shaft", "socket", "pommel", "spear"]),
        ("short-spear", &["shaft", "socket", "pommel", "spear"]),
        (
            "partisan",
            &["shaft", "socket", "pommel", "partisan", "box", "box"],
        ),
        (
            "glaive",
            &["shaft", "socket", "pommel", "glaive", "box", "box"],
        ),
        (
            "hooked-bill",
            &["shaft", "socket", "pommel", "bill", "box", "box"],
        ),
        (
            "military-fork",
            &["shaft", "socket", "pommel", "fork", "box", "box"],
        ),
        (
            "landsknecht-longsword",
            &["grip", "pommel", "guard", "sectionBlade"],
        ),
        (
            "zweihander",
            &["grip", "pommel", "guard", "sectionBlade", "guard"],
        ),
        (
            "katzbalger",
            &["grip", "fanPommel", "figureEight", "sectionBlade"],
        ),
        (
            "grosse-messer",
            &["slabGrip", "pommel", "guard", "blade", "tube", "pommel"],
        ),
        ("dussack", &["grip", "pommel", "knuckleBow", "blade"]),
        ("estoc", &["grip", "pommel", "guard", "sectionBlade"]),
        (
            "rondel-dagger",
            &["grip", "pommel", "pommel", "sectionBlade"],
        ),
        (
            "reitschwert-1540",
            &[
                "grip",
                "pommel",
                "guard",
                "sectionBlade",
                "ringGuard",
                "knuckleBow",
                "pommel",
                "pommel",
                "pommel",
                "pommel",
            ],
        ),
        (
            "reiter-war-hammer",
            &["shaft", "socket", "pommel", "hammer", "facetedBeak"],
        ),
        ("hand-axe", &["shaft", "socket", "axe", "pommel"]),
        (
            "flanged-mace",
            &["grip", "shaft", "collar", "collar", "sleeve", "mace"],
        ),
        (
            "gothic-flanged-mace",
            &["grip", "shaft", "collar", "collar", "sleeve", "mace"],
        ),
    ];
    for (id, expected) in fixture {
        let design = preset_design(id).unwrap();
        let mut actual: Vec<_> = design.components.iter().map(structural_kind).collect();
        let mut expected = expected.to_vec();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "{id}");
    }
    for (id, component, length) in [
        ("halberd-1540", "spike", 320),
        ("lucerne-hammer", "spike", 310),
        ("pollaxe", "spike", 250),
        ("kriegsspiess", "spike", 250),
        ("short-spear", "spike", 310),
        ("partisan", "partisan", 420),
        ("glaive", "glaive", 540),
        ("hooked-bill", "bill", 380),
        ("military-fork", "fork", 390),
        ("landsknecht-longsword", "blade", 1020),
        ("zweihander", "blade", 1280),
        ("katzbalger", "blade", 660),
        ("grosse-messer", "blade", 840),
        ("dussack", "blade", 690),
        ("estoc", "blade", 1050),
        ("rondel-dagger", "blade", 380),
        ("reitschwert-1540", "blade", 880),
        ("reiter-war-hammer", "shaft", 580),
        ("hand-axe", "shaft", 670),
        ("flanged-mace", "head", 148),
        ("gothic-flanged-mace", "head", 265),
    ] {
        let design = preset_design(id).unwrap();
        assert_eq!(
            design
                .components
                .iter()
                .find(|part| part.id == component)
                .unwrap()
                .shape
                .axial_length()
                .0,
            length,
            "{id}.{component}"
        );
    }

    let katz = preset_design("katzbalger").unwrap();
    let ComponentShape::SectionBlade(katz_blade) = &katz
        .components
        .iter()
        .find(|part| part.id == "blade")
        .unwrap()
        .shape
    else {
        panic!()
    };
    assert_eq!(
        (katz_blade.length.0, katz_blade.width.0, katz_blade.taper.0),
        (660, 70, 500)
    );
    assert!(
        matches!(&katz.components.iter().find(|part| part.id == "pommel").unwrap().shape, ComponentShape::FanPommel(v) if (v.width.0,v.height.0,v.thickness.0)==(55,45,14))
    );
    let bill = preset_design("hooked-bill").unwrap();
    assert!(
        matches!(&bill.components.iter().find(|part| part.id == "bill").unwrap().shape, ComponentShape::Bill(v) if (v.belly_position.0,v.point_length.0,v.root_length.0)==(480,240,60))
    );
    let fork = preset_design("military-fork").unwrap();
    assert!(
        matches!(&fork.components.iter().find(|part| part.id == "fork").unwrap().shape, ComponentShape::Fork(v) if (v.shoulder_blend.0,v.crotch_round.0)==(200,50))
    );
    let zweihander = preset_design("zweihander").unwrap();
    assert!(
        zweihander
            .components
            .iter()
            .any(|part| part.id == "parrying-lugs")
    );
    let messer = preset_design("grosse-messer").unwrap();
    assert!(messer.components.iter().any(|part| part.id == "nagel-stem"));
    assert!(
        messer
            .components
            .iter()
            .any(|part| part.id == "nagel-button")
    );
    let halberd = preset_design("halberd-1540").unwrap();
    assert!(
        matches!(&halberd.components.iter().find(|part| part.id == "axe").unwrap().shape, ComponentShape::Axe(v) if (v.upper_shoulder.0,v.lower_shoulder.0,v.flare.0,v.toe.0,v.heel.0,v.beard_drop.0)==(380,260,0,0,0,189))
    );
    let lucerne = preset_design("lucerne-hammer").unwrap();
    assert!(
        matches!(&lucerne.components.iter().find(|part| part.id == "poll").unwrap().shape, ComponentShape::HammerPoll(v) if (v.neck_ratio.0,v.face_flare.0,v.crown_length.0,v.face_thickness.0)==(720,0,5,70))
    );
    let glaive = preset_design("glaive").unwrap();
    assert!(
        matches!(&glaive.components.iter().find(|part| part.id == "glaive").unwrap().shape, ComponentShape::Glaive(v) if (v.belly_position.0,v.point_length.0,v.root_length.0)==(420,240,80))
    );
    let partisan = preset_design("partisan").unwrap();
    assert!(
        matches!(&partisan.components.iter().find(|part| part.id == "partisan").unwrap().shape, ComponentShape::Partisan(v) if (v.lug_drop.0,v.belly_position.0,v.lug_sweep.0,v.acuteness.0)==(75,320,55,1000))
    );

    let cylinder = |design: &WeaponDesign, id: &str| {
        let ComponentShape::Cylinder(value) = &design
            .components
            .iter()
            .find(|part| part.id == id)
            .unwrap()
            .shape
        else {
            panic!("{id} was not a cylinder")
        };
        (value.bottom_scale.0, value.top_scale.0)
    };
    assert_eq!(cylinder(&halberd, "shaft"), (900, 920));
    let longsword = preset_design("landsknecht-longsword").unwrap();
    assert_eq!(cylinder(&longsword, "grip"), (1000, 920));
    let gothic = preset_design("gothic-flanged-mace").unwrap();
    assert_eq!(cylinder(&gothic, "grip"), (1000, 980));
    assert_eq!(cylinder(&gothic, "shaft"), (1000, 940));
    assert_eq!(
        gothic
            .components
            .iter()
            .find(|part| part.id == "grip")
            .unwrap()
            .material,
        MaterialClass::DarkLeather
    );
    for id in ["grosse-messer", "dussack"] {
        let design = preset_design(id).unwrap();
        assert_eq!(
            design
                .components
                .iter()
                .find(|part| part.id == "pommel")
                .unwrap()
                .material,
            MaterialClass::Brass,
            "{id} pommel"
        );
    }
}

#[test]
fn mace_assemblies_have_reference_bottom_to_top_anchor_order() {
    for id in ["flanged-mace", "gothic-flanged-mace"] {
        let generated = generate(&preset_design(id).unwrap()).unwrap();
        let anchor = |name: &str| {
            generated
                .anchors
                .iter()
                .find(|anchor| anchor.name == name)
                .unwrap()
                .position[1]
        };
        assert!(anchor("grip.base") <= anchor("lower-collar.base"), "{id}");
        assert!(anchor("grip.top") <= anchor("shaft.base"), "{id}");
        assert!(anchor("shaft.top") <= anchor("head-sleeve.top"), "{id}");
        assert!(anchor("head-sleeve.base") <= anchor("head.base"), "{id}");
    }
}

#[test]
fn katzbalger_fan_has_sampled_mushroom_dome_and_narrow_neck() {
    let generated = generate(&preset_design("katzbalger").unwrap()).unwrap();
    let fan = generated
        .parts
        .iter()
        .find(|part| part.component_id == "pommel")
        .unwrap();
    assert!(
        fan.positions.len() >= 100,
        "fan silhouette sampling regressed"
    );
    assert!(((fan.bounds.max[0] - fan.bounds.min[0]) - 0.055).abs() < 0.001);
    assert!(((fan.bounds.max[1] - fan.bounds.min[1]) - 0.045).abs() < 0.001);
    let base_y = fan.bounds.min[1];
    let neck = fan
        .positions
        .iter()
        .filter(|point| point[1] - base_y < 0.001)
        .map(|point| point[0].abs())
        .fold(0.0_f32, f32::max);
    assert!(neck <= 0.011, "fan neck widened into a diamond: {neck}");
    assert!(
        fan.positions
            .iter()
            .filter(|point| point[1] - base_y > 0.018)
            .count()
            >= 48,
        "domed cap undersampled"
    );
}

#[test]
fn hostile_full_integer_validation_is_total_and_never_panics() {
    fn poison(shape: &mut ComponentShape) {
        match shape {
            ComponentShape::Cylinder(v) => v.length.0 = u32::MAX,
            ComponentShape::Blade(v) => v.width.0 = u32::MAX,
            ComponentShape::Guard(v) => v.radius.0 = u32::MAX,
            ComponentShape::Mace(v) => v.length.0 = u32::MAX,
            ComponentShape::Socket(v) => v.outer_radius.0 = u32::MAX,
            ComponentShape::Langet(v) => v.length.0 = u32::MAX,
            ComponentShape::SectionBlade(v) => v.length.0 = u32::MAX,
            ComponentShape::Axe(v) => v.reach.0 = u32::MAX,
            ComponentShape::HammerPoll(v) => v.length.0 = u32::MAX,
            ComponentShape::CurvedBeak(v) => v.length.0 = u32::MAX,
            ComponentShape::FacetedBeak(v) => v.length.0 = u32::MAX,
            ComponentShape::Glaive(v) => v.length.0 = u32::MAX,
            ComponentShape::Bill(v) => v.width.0 = u32::MAX,
            ComponentShape::Fork(v) => v.tine_width.0 = u32::MAX,
            ComponentShape::Partisan(v) => v.length.0 = u32::MAX,
            ComponentShape::TubePath(v) => v.radius.0 = u32::MAX,
            ComponentShape::RingGuard(v) => v.radius.0 = u32::MAX,
            ComponentShape::FigureEight(v) => v.bar.0 = u32::MAX,
            ComponentShape::FanPommel(v) => v.width.0 = u32::MAX,
            ComponentShape::Rondel(v) => v.radius.0 = u32::MAX,
            ComponentShape::GothicMace(v) => {
                v.length.0 = u32::MAX;
                v.crown_length.0 = u32::MAX;
            }
            ComponentShape::SlabGrip(v) => v.scale_thickness.0 = u32::MAX,
            ComponentShape::KnuckleBow(v) => v.width.0 = u32::MAX,
            ComponentShape::Collar(v) => v.radius.0 = u32::MAX,
            ComponentShape::Sleeve(v) => v.length.0 = u32::MAX,
            ComponentShape::Boss(v) => v.radius.0 = u32::MAX,
            ComponentShape::Spear(v) => v.length.0 = u32::MAX,
            ComponentShape::ProfiledPommel(v) => v.profile[0].radius.0 = u32::MAX,
        }
    }
    for id in PRESET_IDS {
        let baseline = preset_design(id).unwrap();
        for index in 0..baseline.components.len() {
            let mut hostile = baseline.clone();
            poison(&mut hostile.components[index].shape);
            let result = std::panic::catch_unwind(|| validate(&hostile));
            assert!(
                matches!(result, Ok(Err(_))),
                "{id} component {index}: {result:?}"
            );
        }
    }

    let mut required = preset_design("hand-axe").unwrap();
    let ComponentShape::Axe(axe) = &mut required
        .components
        .iter_mut()
        .find(|part| part.id == "axe")
        .unwrap()
        .shape
    else {
        panic!()
    };
    axe.reach.0 = u32::MAX;
    assert!(matches!(
        std::panic::catch_unwind(|| validate(&required)),
        Ok(Err(_))
    ));
    let mut required = preset_design("gothic-flanged-mace").unwrap();
    let ComponentShape::GothicMace(mace) = &mut required
        .components
        .iter_mut()
        .find(|part| part.id == "head")
        .unwrap()
        .shape
    else {
        panic!()
    };
    mace.length.0 = u32::MAX;
    mace.crown_length.0 = u32::MAX;
    assert!(matches!(
        std::panic::catch_unwind(|| validate(&required)),
        Ok(Err(_))
    ));
    let mut required = preset_design("military-fork").unwrap();
    let ComponentShape::Fork(fork) = &mut required
        .components
        .iter_mut()
        .find(|part| part.id == "fork")
        .unwrap()
        .shape
    else {
        panic!()
    };
    fork.tine_width.0 = u32::MAX;
    assert!(matches!(
        std::panic::catch_unwind(|| validate(&required)),
        Ok(Err(_))
    ));
    let mut required = preset_design("katzbalger").unwrap();
    let ComponentShape::FigureEight(guard) = &mut required
        .components
        .iter_mut()
        .find(|part| part.id == "guard")
        .unwrap()
        .shape
    else {
        panic!()
    };
    guard.bar.0 = u32::MAX;
    assert!(matches!(
        std::panic::catch_unwind(|| validate(&required)),
        Ok(Err(_))
    ));

    let mut required = default_design("walking_staff").unwrap();
    let ComponentShape::Cylinder(shaft) = &mut required.components[0].shape else {
        panic!()
    };
    shaft.bottom_scale.0 = u16::MAX;
    shaft.top_scale.0 = u16::MAX;
    assert!(matches!(
        std::panic::catch_unwind(|| validate(&required)),
        Ok(Err(_))
    ));
}

#[test]
fn tapered_cylinders_affect_mesh_and_derived_mass() {
    let tapered = default_design("walking_staff").unwrap();
    let tapered_generated = generate(&tapered).unwrap();
    let tapered_mass = derive_properties(&tapered).unwrap().mass_kg;

    let mut straight = tapered.clone();
    let ComponentShape::Cylinder(shaft) = &mut straight.components[0].shape else {
        panic!()
    };
    shaft.bottom_scale = Permille(1000);
    shaft.top_scale = Permille(1000);
    validate(&straight).unwrap();
    let straight_generated = generate(&straight).unwrap();
    let straight_mass = derive_properties(&straight).unwrap().mass_kg;

    let tapered_part = &tapered_generated.parts[0];
    let straight_part = &straight_generated.parts[0];
    let tapered_bottom = tapered_part
        .positions
        .iter()
        .filter(|point| (point[1] - tapered_part.bounds.min[1]).abs() < 1e-6)
        .map(|point| point[0].hypot(point[2]))
        .fold(0.0_f32, f32::max);
    let tapered_top = tapered_part
        .positions
        .iter()
        .filter(|point| (point[1] - tapered_part.bounds.max[1]).abs() < 1e-6)
        .map(|point| point[0].hypot(point[2]))
        .fold(0.0_f32, f32::max);
    assert!(
        tapered_top > tapered_bottom,
        "shaft taper was not generated"
    );
    assert!(straight_mass > tapered_mass, "frustum volume was ignored");
    assert!(
        straight_part.bounds.max[0] > tapered_part.bounds.max[0],
        "scale did not affect mesh bounds"
    );
}

#[test]
fn shape_aware_normals_smooth_walls_but_split_caps_and_blade_creases() {
    fn groups(part: &adventuresim_weapon_model::MeshPart) -> HashMap<[u32; 3], Vec<[f32; 3]>> {
        let mut groups = HashMap::<[u32; 3], Vec<[f32; 3]>>::new();
        for (position, normal) in part.positions.iter().zip(&part.normals) {
            groups
                .entry(position.map(f32::to_bits))
                .or_default()
                .push(*normal);
        }
        groups
    }
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    let staff = generate(&default_design("walking_staff").unwrap()).unwrap();
    let shaft = &staff.parts[0];
    let shaft_groups = groups(shaft);
    assert!(
        shaft_groups.values().any(|normals| {
            normals.iter().any(|normal| normal[1].abs() > 0.99)
                && normals.iter().any(|normal| normal[1].abs() < 0.2)
        }),
        "cylinder cap seam was smoothed"
    );
    for (position, normal) in shaft.positions.iter().zip(&shaft.normals) {
        if (position[1] - shaft.bounds.min[1]).abs() > 1e-5
            && (position[1] - shaft.bounds.max[1]).abs() > 1e-5
        {
            let radial = [position[0], 0.0, position[2]];
            let magnitude = radial[0].hypot(radial[2]);
            assert!(magnitude > 0.0);
            assert!(
                dot(*normal, [radial[0] / magnitude, 0.0, radial[2] / magnitude]) > 0.9,
                "curved shaft wall was not radially smooth: {normal:?}"
            );
        }
    }

    let bill = generate(&preset_design("hooked-bill").unwrap()).unwrap();
    let bill = bill
        .parts
        .iter()
        .find(|part| part.component_id == "bill")
        .unwrap();
    assert!(
        groups(bill).values().any(|normals| {
            normals
                .iter()
                .enumerate()
                .any(|(index, a)| normals[index + 1..].iter().any(|b| dot(*a, *b) < 0.25))
        }),
        "blade front/side crease was globally averaged"
    );
}
