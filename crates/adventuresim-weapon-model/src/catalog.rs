use crate::*;

pub const PRESET_IDS: &[&str] = &[
    "halberd-1540",
    "lucerne-hammer",
    "pollaxe",
    "kriegsspiess",
    "short-spear",
    "partisan",
    "glaive",
    "hooked-bill",
    "military-fork",
    "landsknecht-longsword",
    "zweihander",
    "katzbalger",
    "grosse-messer",
    "dussack",
    "estoc",
    "rondel-dagger",
    "reitschwert-1540",
    "reiter-war-hammer",
    "hand-axe",
    "flanged-mace",
    "gothic-flanged-mace",
];
pub const MELEE_CATALOG_IDS: &[&str] = &[
    "arming_sword",
    "baselard",
    "bauernwehr",
    "club",
    "falchion",
    "flanged_mace",
    "halberd",
    "hand_axe",
    "hunting_spear",
    "katzbalger",
    "knife",
    "kriegsmesser",
    "longsword",
    "messer",
    "military_pike",
    "misericorde",
    "rapier",
    "rondel_dagger",
    "spear",
    "utility_knife",
    "walking_staff",
    "war_hammer",
    "zweihander",
];

pub fn recommended_holder(catalog_id: &str) -> Option<WeaponHolderKind> {
    match catalog_id {
        "arming_sword" | "baselard" | "bauernwehr" | "falchion" | "katzbalger" | "knife"
        | "kriegsmesser" | "longsword" | "messer" | "misericorde" | "rapier" | "rondel_dagger"
        | "utility_knife" | "zweihander" => Some(WeaponHolderKind::BladeSheath),
        "club" | "flanged_mace" | "hand_axe" | "war_hammer" => Some(WeaponHolderKind::HaftLoop),
        "halberd" | "hunting_spear" | "military_pike" | "spear" | "walking_staff" => None,
        _ => None,
    }
}

pub fn default_holder_design(weapon: &WeaponDesign) -> Option<WeaponHolderDesign> {
    let kind = recommended_holder(&weapon.catalog_id)?;
    Some(WeaponHolderDesign {
        catalog_id: match kind {
            WeaponHolderKind::BladeSheath => "scabbard",
            WeaponHolderKind::HaftLoop => "weapon_loop",
        }
        .into(),
        kind,
        fitted_weapon: weapon.clone(),
        body_material: MaterialClass::DarkLeather,
        fitting_material: MaterialClass::Brass,
        clearance: Millimeters(5),
        throat_length: Millimeters(12),
        chape_length: Millimeters(20),
        loop_position: Permille(280),
        loop_bar_radius: Millimeters(4),
        hanger_width: Millimeters(42),
        hanger_height: Millimeters(76),
    })
}

fn component(
    id: &str,
    role: ComponentRole,
    parent: Option<(&str, u32)>,
    offset: OffsetMm,
    material: MaterialClass,
    shape: ComponentShape,
) -> ComponentDesign {
    ComponentDesign {
        id: id.into(),
        role,
        attachment: parent.map_or(Attachment::Root, |(name, insertion)| Attachment::TopOf {
            component: name.into(),
            insertion: Millimeters(insertion),
        }),
        offset,
        material,
        shape,
    }
}
fn shaft(length: u32, radius: u32, steel: bool) -> ComponentDesign {
    component(
        "shaft",
        ComponentRole::Grip,
        None,
        OffsetMm::default(),
        if steel {
            MaterialClass::Steel
        } else {
            MaterialClass::Wood
        },
        ComponentShape::Cylinder(CylinderSpec {
            length: Millimeters(length),
            radius: Millimeters(radius),
            bottom_scale: Permille(900),
            top_scale: Permille(920),
            segments: Segments(16),
        }),
    )
}
fn socket(parent: &str, length: u32, radius: u32, insertion: u32) -> ComponentDesign {
    component(
        "socket",
        ComponentRole::Socket,
        Some((parent, insertion)),
        OffsetMm::default(),
        MaterialClass::Steel,
        ComponentShape::Socket(SocketSpec {
            length: Millimeters(length),
            outer_radius: Millimeters(radius),
            top_radius: Millimeters(radius.saturating_sub(3)),
            wall: Millimeters(4),
            segments: Segments(18),
        }),
    )
}
fn langet(id: &str, x: i32, length: u32) -> ComponentDesign {
    component(
        id,
        ComponentRole::Structure,
        Some(("shaft", length.min(380))),
        OffsetMm { x, y: 0, z: 0 },
        MaterialClass::Steel,
        ComponentShape::Langet(LangetSpec {
            length: Millimeters(length),
            width: Millimeters(13),
            thickness: Millimeters(5),
        }),
    )
}
fn spear(
    id: &str,
    parent: &str,
    length: u32,
    width: u32,
    thickness: u32,
    belly: u16,
) -> ComponentDesign {
    component(
        id,
        ComponentRole::Head,
        Some((parent, 10)),
        OffsetMm::default(),
        MaterialClass::Steel,
        ComponentShape::Spear(SpearSpec {
            length: Millimeters(length),
            width: Millimeters(width),
            thickness: Millimeters(thickness),
            root_width: Millimeters(width * 2 / 5),
            belly_position: Permille(belly),
            acuteness: Permille(1000),
            samples: Segments(18),
        }),
    )
}
fn polearm_base(
    id: &str,
    length: u32,
    radius: u32,
    socket_length: u32,
    socket_radius: u32,
) -> WeaponDesign {
    let shaft_segments = if matches!(
        id,
        "halberd-1540" | "lucerne-hammer" | "pollaxe" | "hooked-bill"
    ) {
        Segments(8)
    } else {
        Segments(16)
    };
    let mut haft = shaft(length, radius, false);
    if let ComponentShape::Cylinder(spec) = &mut haft.shape {
        spec.segments = shaft_segments;
    }
    WeaponDesign {
        catalog_id: id.into(),
        components: vec![
            haft,
            socket("shaft", socket_length, socket_radius, socket_length * 3 / 4),
            component(
                "butt-cap",
                ComponentRole::Structure,
                Some(("shaft", length)),
                OffsetMm { x: 0, y: -40, z: 0 },
                MaterialClass::DarkSteel,
                ComponentShape::Cylinder(CylinderSpec {
                    length: Millimeters(40),
                    radius: Millimeters(radius),
                    bottom_scale: Permille(1000),
                    top_scale: Permille(920),
                    segments: Segments(16),
                }),
            ),
        ],
    }
}
fn polearm_finish(mut design: WeaponDesign, length: u32) -> WeaponDesign {
    design.components.push(langet("langet-left", -18, length));
    design.components.push(langet("langet-right", 18, length));
    design
}

// This constructor keeps the authored dimensions and component shapes at the
// call site; bundling them into an options struct would obscure the catalog.
#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn sword(
    id: &str,
    blade_length: u32,
    width: u32,
    grip_length: u32,
    section: BladeSection,
    curvature: i32,
    guard: ComponentShape,
    pommel: ComponentShape,
) -> WeaponDesign {
    let grip = component(
        "grip",
        ComponentRole::Grip,
        None,
        OffsetMm::default(),
        MaterialClass::Leather,
        ComponentShape::OvalGrip(OvalGripSpec {
            length: Millimeters(grip_length),
            width: Millimeters(32),
            thickness: Millimeters(23),
            bottom_scale: Permille(1000),
            top_scale: Permille(850),
            segments: Segments(16),
        }),
    );
    let pommel_height = pommel.axial_length().0;
    let pommel = component(
        "pommel",
        ComponentRole::Structure,
        Some(("grip", grip_length)),
        OffsetMm {
            x: 0,
            y: -(pommel_height as i32),
            z: 0,
        },
        MaterialClass::Steel,
        pommel,
    );
    let guard = component(
        "guard",
        ComponentRole::Guard,
        Some(("grip", 0)),
        OffsetMm::default(),
        MaterialClass::Steel,
        guard,
    );
    let blade = component(
        "blade",
        ComponentRole::Head,
        Some(("guard", 0)),
        OffsetMm::default(),
        MaterialClass::Steel,
        ComponentShape::Blade(BladeSpec {
            length: Millimeters(blade_length),
            width: Millimeters(width),
            thickness: Millimeters(12),
            curvature: SignedMillimeters(curvature),
            profile: BladeProfile::Straight,
            section,
            samples: Segments(24),
            taper: Permille(820),
            single_edge: Permille(0),
            belly: SignedPermille(0),
            ricasso: Millimeters(0),
        }),
    );
    WeaponDesign {
        catalog_id: id.into(),
        components: vec![grip, pommel, guard, blade],
    }
}
fn cross(span: u32, sweep: i32) -> ComponentShape {
    ComponentShape::Guard(GuardSpec {
        span: Millimeters(span),
        radius: Millimeters(8),
        sweep: SignedMillimeters(sweep),
        samples: Segments(22),
        radial_segments: Segments(14),
    })
}
fn set_oval_grip(design: &mut WeaponDesign, width: u32, thickness: u32) {
    if let ComponentShape::OvalGrip(grip) = &mut design.components[0].shape {
        grip.width = Millimeters(width);
        grip.thickness = Millimeters(thickness);
    }
}
fn profiled(points: &[(u32, u32)]) -> ComponentShape {
    ComponentShape::ProfiledPommel(ProfiledPommelSpec {
        profile: points
            .iter()
            .map(|(y, radius)| ProfilePointMm {
                y: Millimeters(*y),
                radius: Millimeters(*radius),
            })
            .collect(),
        segments: Segments(16),
    })
}

fn plain_blade_furniture(
    mut design: WeaponDesign,
    guard_span: u32,
    pommel_radius: u32,
) -> WeaponDesign {
    if let Some(guard) = design
        .components
        .iter_mut()
        .find(|component| component.id == "guard")
    {
        guard.shape = cross(guard_span, 0);
    }
    if let Some(pommel) = design
        .components
        .iter_mut()
        .find(|component| component.id == "pommel")
    {
        pommel.shape = profiled(&[
            (0, pommel_radius),
            (18, pommel_radius + 5),
            (36, pommel_radius),
        ]);
        pommel.offset.y = -(pommel.shape.axial_length().0 as i32);
    }
    design
}
fn curved_blade(
    length: u32,
    width: u32,
    thickness: u32,
    curvature: i32,
    taper: u16,
    single_edge: u16,
    belly: i16,
) -> ComponentShape {
    ComponentShape::Blade(BladeSpec {
        length: Millimeters(length),
        width: Millimeters(width),
        thickness: Millimeters(thickness),
        curvature: SignedMillimeters(curvature),
        profile: BladeProfile::Curved,
        section: BladeSection::Flat,
        samples: Segments(24),
        taper: Permille(taper),
        single_edge: Permille(single_edge),
        belly: SignedPermille(belly),
        ricasso: Millimeters(0),
    })
}
fn gothic(id: &str, length: u32, haft: u32, concavity: u16) -> WeaponDesign {
    let (
        haft_radius,
        grip_length,
        grip_radius,
        collar_width,
        collar_radius,
        sleeve_length,
        root_radius,
        shoulder_radius,
        crown_length,
        flange_thickness,
    ) = if concavity > 500 {
        (11, 200, 19, 18, 22, 150, 9, 7, 15, 2)
    } else {
        (13, 170, 20, 14, 23, 90, 8, 7, 8, 3)
    };
    WeaponDesign {
        catalog_id: id.into(),
        components: vec![
            component(
                "grip",
                ComponentRole::Grip,
                None,
                OffsetMm::default(),
                if concavity > 500 {
                    MaterialClass::DarkLeather
                } else {
                    MaterialClass::Leather
                },
                ComponentShape::Cylinder(CylinderSpec {
                    length: Millimeters(grip_length),
                    radius: Millimeters(grip_radius),
                    bottom_scale: Permille(1000),
                    top_scale: Permille(980),
                    segments: Segments(16),
                }),
            ),
            component(
                "shaft",
                ComponentRole::Structure,
                Some(("grip", 0)),
                OffsetMm::default(),
                MaterialClass::DarkSteel,
                ComponentShape::Cylinder(CylinderSpec {
                    length: Millimeters(haft),
                    radius: Millimeters(haft_radius),
                    bottom_scale: Permille(1000),
                    top_scale: Permille(940),
                    segments: Segments(16),
                }),
            ),
            component(
                "lower-collar",
                ComponentRole::Structure,
                Some(("grip", grip_length)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Collar(CollarSpec {
                    width: Millimeters(collar_width),
                    radius: Millimeters(collar_radius),
                    segments: Segments(16),
                }),
            ),
            component(
                "upper-collar",
                ComponentRole::Structure,
                Some(("grip", 0)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Collar(CollarSpec {
                    width: Millimeters(collar_width),
                    radius: Millimeters(collar_radius),
                    segments: Segments(16),
                }),
            ),
            component(
                "head-sleeve",
                ComponentRole::Socket,
                Some(("shaft", 12)),
                OffsetMm::default(),
                MaterialClass::DarkSteel,
                ComponentShape::Sleeve(SleeveSpec {
                    length: Millimeters(sleeve_length),
                    radius: Millimeters(root_radius + 2),
                    top_radius: Millimeters(root_radius),
                    wall: Millimeters(3),
                    segments: Segments(16),
                }),
            ),
            component(
                "head",
                ComponentRole::Head,
                Some(("head-sleeve", 12)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::GothicMace(GothicMaceSpec {
                    length: Millimeters(length),
                    root_radius: Millimeters(root_radius),
                    shoulder_radius: Millimeters(shoulder_radius),
                    cusp_radius: Millimeters(60),
                    cusp_height: Permille(if concavity > 500 { 750 } else { 500 }),
                    concavity: Permille(concavity),
                    crown_length: Millimeters(crown_length),
                    flanges: 6,
                    flange_thickness: Millimeters(flange_thickness),
                    profile_samples: Segments(14),
                    radial_segments: Segments(18),
                }),
            ),
        ],
    }
}

// The wildcard arm exits the outer `Option`-returning function from inside the
// `Some`-wrapped match.
pub fn preset_design(id: &str) -> Option<WeaponDesign> {
    Some(match id {
        "halberd-1540" => {
            let mut d = polearm_base(id, 1820, 22, 240, 33);
            d.components.push(component(
                "axe",
                ComponentRole::Head,
                Some(("socket", 30)),
                OffsetMm { x: 0, y: 15, z: 0 },
                MaterialClass::Steel,
                ComponentShape::Axe(AxeSpec {
                    reach: Millimeters(155),
                    height: Millimeters(270),
                    thickness: Millimeters(22),
                    root_width: Millimeters(28),
                    beard: Permille(420),
                    curvature: Permille(80),
                    side: 1,
                    upper_shoulder: Permille(380),
                    lower_shoulder: Permille(260),
                    flare: SignedPermille(0),
                    toe: SignedPermille(0),
                    heel: SignedPermille(0),
                    beard_drop: Permille(189),
                }),
            ));
            d.components.push(component(
                "beak",
                ComponentRole::Head,
                Some(("socket", 70)),
                OffsetMm {
                    x: -15,
                    y: 70,
                    z: 0,
                },
                MaterialClass::Steel,
                ComponentShape::CurvedBeak(CurvedBeakSpec {
                    length: Millimeters(100),
                    root_section: Millimeters(33),
                    tip_section: Millimeters(3),
                    thickness: Millimeters(18),
                    curvature: SignedMillimeters(-40),
                    direction: -1,
                    samples: Segments(18),
                    bend_position: Permille(550),
                    droop: SignedMillimeters(-14),
                }),
            ));
            d.components
                .push(spear("spike", "socket", 320, 60, 22, 140));
            polearm_finish(d, 380)
        }
        "lucerne-hammer" => {
            let mut d = polearm_base(id, 1740, 22, 240, 33);
            d.components.push(component(
                "poll",
                ComponentRole::Head,
                Some(("socket", 45)),
                OffsetMm { x: 0, y: 45, z: 0 },
                MaterialClass::Steel,
                ComponentShape::HammerPoll(HammerPollSpec {
                    length: Millimeters(90),
                    face: Millimeters(75),
                    neck: Millimeters(46),
                    thickness: Millimeters(70),
                    direction: 1,
                    crown: Permille(60),
                    neck_ratio: Permille(720),
                    face_flare: Permille(0),
                    crown_length: Millimeters(5),
                    face_thickness: Millimeters(70),
                }),
            ));
            d.components.push(component(
                "beak",
                ComponentRole::Head,
                Some(("socket", 45)),
                OffsetMm { x: -8, y: 45, z: 0 },
                MaterialClass::Steel,
                ComponentShape::CurvedBeak(CurvedBeakSpec {
                    length: Millimeters(140),
                    root_section: Millimeters(36),
                    tip_section: Millimeters(3),
                    thickness: Millimeters(18),
                    curvature: SignedMillimeters(25),
                    direction: -1,
                    samples: Segments(20),
                    bend_position: Permille(550),
                    droop: SignedMillimeters(9),
                }),
            ));
            d.components
                .push(spear("spike", "socket", 310, 52, 22, 120));
            polearm_finish(d, 420)
        }
        "pollaxe" => {
            let mut d = polearm_base(id, 1480, 22, 250, 32);
            d.components.push(component(
                "axe",
                ComponentRole::Head,
                Some(("socket", 40)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Axe(AxeSpec {
                    reach: Millimeters(115),
                    height: Millimeters(180),
                    thickness: Millimeters(23),
                    root_width: Millimeters(30),
                    beard: Permille(80),
                    curvature: Permille(25),
                    side: 1,
                    upper_shoulder: Permille(380),
                    lower_shoulder: Permille(260),
                    flare: SignedPermille(0),
                    toe: SignedPermille(0),
                    heel: SignedPermille(0),
                    beard_drop: Permille(40),
                }),
            ));
            d.components.push(component(
                "poll",
                ComponentRole::Head,
                Some(("socket", 40)),
                OffsetMm { x: 0, y: 40, z: 0 },
                MaterialClass::Steel,
                ComponentShape::HammerPoll(HammerPollSpec {
                    length: Millimeters(90),
                    face: Millimeters(65),
                    neck: Millimeters(40),
                    thickness: Millimeters(68),
                    direction: -1,
                    crown: Permille(80),
                    neck_ratio: Permille(720),
                    face_flare: Permille(0),
                    crown_length: Millimeters(7),
                    face_thickness: Millimeters(68),
                }),
            ));
            d.components
                .push(spear("spike", "socket", 250, 48, 22, 110));
            polearm_finish(d, 500)
        }
        "kriegsspiess" => {
            let mut d = polearm_base(id, 3350, 19, 180, 27);
            d.components
                .push(spear("spike", "socket", 250, 43, 22, 150));
            d
        }
        "short-spear" => {
            let mut d = polearm_base(id, 1720, 20, 200, 31);
            d.components
                .push(spear("spike", "socket", 310, 85, 24, 320));
            d
        }
        "partisan" => {
            let mut d = polearm_base(id, 1780, 21, 220, 34);
            d.components.push(component(
                "partisan",
                ComponentRole::Head,
                Some(("socket", 15)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Partisan(PartisanSpec {
                    length: Millimeters(420),
                    width: Millimeters(135),
                    lug_width: Millimeters(145),
                    thickness: Millimeters(22),
                    belly: Permille(320),
                    root_width: Millimeters(24),
                    lug_drop: Permille(75),
                    belly_position: Permille(320),
                    lug_sweep: Permille(55),
                    acuteness: Permille(1000),
                }),
            ));
            polearm_finish(d, 320)
        }
        "glaive" => {
            let mut d = polearm_base(id, 1720, 21, 250, 34);
            d.components.push(component(
                "glaive",
                ComponentRole::Head,
                Some(("socket", 15)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Glaive(GlaiveSpec {
                    length: Millimeters(540),
                    width: Millimeters(105),
                    thickness: Millimeters(18),
                    curvature: SignedMillimeters(130),
                    root: Millimeters(32),
                    edge_curvature: Permille(240),
                    spine_curvature: Permille(200),
                    point_length: Permille(240),
                    samples: Segments(18),
                    belly_position: Permille(420),
                    root_length: Millimeters(80),
                }),
            ));
            polearm_finish(d, 400)
        }
        "hooked-bill" => {
            let mut d = polearm_base(id, 1830, 22, 230, 33);
            d.components.push(component(
                "bill",
                ComponentRole::Head,
                Some(("socket", 10)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Bill(BillSpec {
                    length: Millimeters(380),
                    width: Millimeters(90),
                    hook: Millimeters(80),
                    thickness: Millimeters(20),
                    root: Millimeters(30),
                    hook_depth: Permille(190),
                    hook_curvature: Permille(220),
                    samples: Segments(30),
                    belly_position: Permille(480),
                    point_length: Permille(240),
                    root_length: Millimeters(60),
                }),
            ));
            polearm_finish(d, 380)
        }
        "military-fork" => {
            let mut d = polearm_base(id, 1860, 21, 240, 34);
            d.components.push(component(
                "fork",
                ComponentRole::Head,
                Some(("socket", 10)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::Fork(ForkSpec {
                    length: Millimeters(390),
                    width: Millimeters(130),
                    base_width: Millimeters(55),
                    thickness: Millimeters(22),
                    tine_width: Millimeters(26),
                    crotch: Permille(340),
                    taper: Permille(550),
                    shoulder_blend: Permille(200),
                    crotch_round: Permille(50),
                }),
            ));
            polearm_finish(d, 340)
        }
        "landsknecht-longsword" => {
            let mut d = sword(
                id,
                1020,
                65,
                300,
                BladeSection::Fullered,
                0,
                cross(310, 18),
                profiled(&[(0, 12), (10, 17), (38, 20), (55, 10)]),
            );
            set_oval_grip(&mut d, 33, 24);
            d
        }
        "zweihander" => {
            let mut d = sword(
                id,
                1280,
                72,
                415,
                BladeSection::Fullered,
                0,
                cross(480, 35),
                profiled(&[(0, 14), (12, 21), (45, 24), (65, 12)]),
            );
            if let ComponentShape::Blade(blade) = &mut d.components[3].shape {
                blade.thickness = Millimeters(13);
                blade.taper = Permille(680);
                blade.ricasso = Millimeters(260);
            }
            set_oval_grip(&mut d, 38, 28);
            d.components.push(component(
                "parrying-lugs",
                ComponentRole::Guard,
                Some(("blade", 1065)),
                OffsetMm::default(),
                MaterialClass::Steel,
                cross(180, 0),
            ));
            d
        }
        "katzbalger" => {
            let mut d = sword(
                id,
                660,
                70,
                165,
                BladeSection::Fullered,
                0,
                ComponentShape::FigureEight(FigureEightSpec {
                    width: Millimeters(140),
                    height: Millimeters(38),
                    bar: Millimeters(7),
                    samples: Segments(48),
                    radial_segments: Segments(14),
                }),
                ComponentShape::FanPommel(FanPommelSpec {
                    width: Millimeters(55),
                    height: Millimeters(45),
                    thickness: Millimeters(14),
                }),
            );
            set_oval_grip(&mut d, 34, 25);
            if let ComponentShape::Blade(blade) = &mut d.components[3].shape {
                blade.thickness = Millimeters(11);
                blade.taper = Permille(500);
            }
            d
        }
        "grosse-messer" => {
            let mut d = sword(
                id,
                840,
                64,
                215,
                BladeSection::Flat,
                85,
                cross(230, 10),
                profiled(&[(0, 16), (18, 22), (38, 14)]),
            );
            d.components[0].material = MaterialClass::Wood;
            d.components[1].material = MaterialClass::Brass;
            d.components[0].shape = ComponentShape::SlabGrip(SlabGripSpec {
                length: Millimeters(215),
                width: Millimeters(38),
                thickness: Millimeters(10),
                scale_thickness: Millimeters(9),
            });
            d.components[3].shape = curved_blade(840, 64, 11, 85, 780, 720, 160);
            d.components.push(component(
                "nagel-stem",
                ComponentRole::Guard,
                Some(("guard", 0)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::TubePath(TubePathSpec {
                    points: vec![OffsetMm::default(), OffsetMm { x: 0, y: 0, z: 45 }],
                    radius: Millimeters(5),
                    radial_segments: Segments(12),
                    closed: false,
                }),
            ));
            d.components.push(component(
                "nagel-button",
                ComponentRole::Guard,
                Some(("nagel-stem", 0)),
                OffsetMm { x: 0, y: 0, z: 45 },
                MaterialClass::Steel,
                ComponentShape::Boss(BossSpec {
                    radius: Millimeters(8),
                    thickness: Millimeters(12),
                    segments: Segments(16),
                }),
            ));
            d
        }
        "dussack" => {
            let mut d = sword(
                id,
                690,
                68,
                155,
                BladeSection::Flat,
                130,
                ComponentShape::KnuckleBow(KnuckleBowSpec {
                    width: Millimeters(100),
                    length: Millimeters(155),
                    bar: Millimeters(10),
                    side: 1,
                    bulge: Permille(0),
                    samples: Segments(24),
                    radial_segments: Segments(14),
                }),
                profiled(&[(0, 13), (15, 18), (32, 11)]),
            );
            d.components[3].shape = curved_blade(690, 68, 12, 130, 820, 800, 320);
            d.components[1].material = MaterialClass::Brass;
            d
        }
        "estoc" => {
            let mut d = sword(
                id,
                1050,
                34,
                260,
                BladeSection::Diamond,
                0,
                cross(240, 5),
                profiled(&[(0, 12), (10, 17), (36, 19), (52, 9)]),
            );
            set_oval_grip(&mut d, 31, 23);
            d
        }
        "rondel-dagger" => {
            let mut d = sword(
                id,
                380,
                32,
                125,
                BladeSection::Diamond,
                0,
                ComponentShape::Rondel(RondelSpec {
                    radius: Millimeters(32),
                    thickness: Millimeters(8),
                    segments: Segments(20),
                }),
                profiled(&[(0, 22), (8, 32), (16, 22)]),
            );
            set_oval_grip(&mut d, 30, 22);
            d
        }
        "reitschwert-1540" => {
            let mut d = sword(
                id,
                880,
                46,
                170,
                BladeSection::Fullered,
                0,
                cross(230, 10),
                profiled(&[(0, 12), (10, 17), (32, 19), (45, 9)]),
            );
            set_oval_grip(&mut d, 31, 22);
            d.components.push(component(
                "side-ring",
                ComponentRole::Guard,
                Some(("guard", 0)),
                OffsetMm { x: 0, y: 0, z: 12 },
                MaterialClass::Steel,
                ComponentShape::RingGuard(RingGuardSpec {
                    radius: Millimeters(55),
                    bar: Millimeters(6),
                    arc_start: MilliRadians(0),
                    arc_end: MilliRadians(3142),
                    samples: Segments(28),
                    radial_segments: Segments(12),
                }),
            ));
            d.components.push(component(
                "knuckle-bow",
                ComponentRole::Guard,
                Some(("grip", 0)),
                OffsetMm::default(),
                MaterialClass::Steel,
                ComponentShape::KnuckleBow(KnuckleBowSpec {
                    width: Millimeters(75),
                    length: Millimeters(170),
                    bar: Millimeters(7),
                    side: 1,
                    bulge: Permille(0),
                    samples: Segments(24),
                    radial_segments: Segments(12),
                }),
            ));
            for (name, x, z, parent) in [
                ("left-side-ring-boss", -55, 12, "guard"),
                ("right-side-ring-boss", 55, 12, "guard"),
                ("upper-knuckle-boss", 0, 0, "guard"),
                ("lower-knuckle-boss", 0, 0, "grip"),
            ] {
                d.components.push(component(
                    name,
                    ComponentRole::Guard,
                    Some((parent, 0)),
                    OffsetMm { x, y: 0, z },
                    MaterialClass::DarkSteel,
                    ComponentShape::Boss(BossSpec {
                        radius: Millimeters(12),
                        thickness: Millimeters(48),
                        segments: Segments(16),
                    }),
                ));
            }
            d
        }
        "reiter-war-hammer" => {
            let mut d = polearm_base(id, 580, 18, 350, 22);
            d.components[0].material = MaterialClass::Steel;
            d.components.push(component(
                "poll",
                ComponentRole::Head,
                Some(("socket", 15)),
                OffsetMm { x: 0, y: 15, z: 0 },
                MaterialClass::Steel,
                ComponentShape::HammerPoll(HammerPollSpec {
                    length: Millimeters(64),
                    face: Millimeters(46),
                    neck: Millimeters(26),
                    thickness: Millimeters(50),
                    direction: 1,
                    crown: Permille(60),
                    neck_ratio: Permille(720),
                    face_flare: Permille(0),
                    crown_length: Millimeters(4),
                    face_thickness: Millimeters(50),
                }),
            ));
            d.components.push(component(
                "beak",
                ComponentRole::Head,
                Some(("socket", 15)),
                OffsetMm { x: 0, y: 15, z: 0 },
                MaterialClass::Steel,
                ComponentShape::FacetedBeak(FacetedBeakSpec {
                    length: Millimeters(75),
                    root: Millimeters(38),
                    tip: Millimeters(8),
                    thickness: Millimeters(14),
                    set: SignedMillimeters(5),
                    direction: -1,
                    bend_position: Permille(220),
                    tip_thickness: Millimeters(14),
                }),
            ));
            d
        }
        "hand-axe" => WeaponDesign {
            catalog_id: id.into(),
            components: vec![
                shaft(670, 21, false),
                socket("shaft", 100, 27, 80),
                component(
                    "axe",
                    ComponentRole::Head,
                    Some(("socket", 20)),
                    OffsetMm::default(),
                    MaterialClass::Steel,
                    ComponentShape::Axe(AxeSpec {
                        reach: Millimeters(180),
                        height: Millimeters(180),
                        thickness: Millimeters(28),
                        root_width: Millimeters(32),
                        beard: Permille(500),
                        curvature: Permille(100),
                        side: 1,
                        upper_shoulder: Permille(380),
                        lower_shoulder: Permille(260),
                        flare: SignedPermille(0),
                        toe: SignedPermille(0),
                        heel: SignedPermille(0),
                        beard_drop: Permille(225),
                    }),
                ),
                component(
                    "butt-cap",
                    ComponentRole::Structure,
                    Some(("shaft", 670)),
                    OffsetMm { x: 0, y: -40, z: 0 },
                    MaterialClass::DarkSteel,
                    ComponentShape::Cylinder(CylinderSpec {
                        length: Millimeters(40),
                        radius: Millimeters(21),
                        bottom_scale: Permille(1000),
                        top_scale: Permille(920),
                        segments: Segments(16),
                    }),
                ),
            ],
        },
        "flanged-mace" => gothic(id, 140, 580, 150),
        "gothic-flanged-mace" => gothic(id, 250, 780, 920),
        _ => return None,
    })
}

// These two catalog-native designs have no reusable preset. Resolve them before
// the configured-preset match so unknown catalog IDs still fail closed.
#[expect(
    clippy::needless_return,
    reason = "the catalog-native branches return before configured preset resolution"
)]
pub fn default_design(catalog_id: &str) -> Option<WeaponDesign> {
    if catalog_id == "walking_staff" {
        return Some(WeaponDesign {
            catalog_id: catalog_id.into(),
            components: vec![shaft(1_850, 22, false)],
        });
    }
    if catalog_id == "club" {
        return Some(WeaponDesign {
            catalog_id: catalog_id.into(),
            components: vec![
                shaft(620, 22, false),
                component(
                    "swollen-head",
                    ComponentRole::Head,
                    Some(("shaft", 90)),
                    OffsetMm::default(),
                    MaterialClass::Wood,
                    ComponentShape::Cylinder(CylinderSpec {
                        length: Millimeters(230),
                        radius: Millimeters(48),
                        bottom_scale: Permille(1000),
                        top_scale: Permille(1000),
                        segments: Segments(18),
                    }),
                ),
            ],
        });
    }
    let configured_blade = |preset: &str,
                            length: u32,
                            width: u32,
                            thickness: u32,
                            profile: BladeProfile,
                            section: BladeSection,
                            taper: u16,
                            single_edge: u16,
                            belly: i16,
                            curvature: i32,
                            ricasso: u32| {
        let mut design = preset_design(preset)?;
        design.catalog_id = catalog_id.into();
        let blade = design.components.iter_mut().find_map(|component| {
            if let ComponentShape::Blade(blade) = &mut component.shape {
                Some(blade)
            } else {
                None
            }
        })?;
        blade.length = Millimeters(length);
        blade.width = Millimeters(width);
        blade.thickness = Millimeters(thickness);
        blade.profile = profile;
        blade.section = section;
        blade.taper = Permille(taper);
        blade.single_edge = Permille(single_edge);
        blade.belly = SignedPermille(belly);
        blade.curvature = SignedMillimeters(curvature);
        blade.ricasso = Millimeters(ricasso);
        Some(design)
    };
    let retagged = |preset: &str| {
        let mut design = preset_design(preset)?;
        design.catalog_id = catalog_id.into();
        Some(design)
    };
    match catalog_id {
        "arming_sword" => configured_blade(
            "landsknecht-longsword",
            820,
            54,
            11,
            BladeProfile::Straight,
            BladeSection::Fullered,
            790,
            0,
            0,
            0,
            0,
        ),
        "baselard" => configured_blade(
            "rondel-dagger",
            430,
            48,
            10,
            BladeProfile::Spear,
            BladeSection::Diamond,
            760,
            0,
            0,
            0,
            0,
        )
        .map(|design| plain_blade_furniture(design, 105, 31)),
        "bauernwehr" => configured_blade(
            "grosse-messer",
            480,
            55,
            9,
            BladeProfile::Curved,
            BladeSection::Flat,
            760,
            760,
            130,
            35,
            0,
        ),
        "falchion" => configured_blade(
            "dussack",
            720,
            74,
            12,
            BladeProfile::Cleaver,
            BladeSection::Flat,
            700,
            900,
            300,
            110,
            0,
        ),
        "flanged_mace" => retagged("flanged-mace"),
        "halberd" => retagged("halberd-1540"),
        "hand_axe" => retagged("hand-axe"),
        "hunting_spear" => {
            let mut design = retagged("short-spear")?;
            if let ComponentShape::Cylinder(shaft) = &mut design.components[0].shape {
                shaft.radius = Millimeters(21);
            }
            if let Some(ComponentShape::Spear(head)) =
                design.components.last_mut().map(|part| &mut part.shape)
            {
                head.length = Millimeters(270);
                head.width = Millimeters(68);
            }
            Some(design)
        }
        "katzbalger" => retagged("katzbalger"),
        "knife" => configured_blade(
            "rondel-dagger",
            210,
            30,
            5,
            BladeProfile::Straight,
            BladeSection::Flat,
            720,
            1000,
            0,
            0,
            0,
        )
        .map(|design| plain_blade_furniture(design, 62, 24)),
        "utility_knife" => configured_blade(
            "rondel-dagger",
            135,
            26,
            4,
            BladeProfile::Cleaver,
            BladeSection::Flat,
            620,
            1000,
            120,
            8,
            0,
        )
        .map(|design| plain_blade_furniture(design, 48, 21)),
        "misericorde" => configured_blade(
            "estoc",
            360,
            24,
            9,
            BladeProfile::Spear,
            BladeSection::Diamond,
            920,
            0,
            0,
            0,
            0,
        )
        .map(|design| plain_blade_furniture(design, 78, 27)),
        "kriegsmesser" => configured_blade(
            "grosse-messer",
            1050,
            72,
            13,
            BladeProfile::Curved,
            BladeSection::Flat,
            730,
            760,
            180,
            105,
            120,
        ),
        "messer" => configured_blade(
            "grosse-messer",
            720,
            58,
            10,
            BladeProfile::Curved,
            BladeSection::Flat,
            780,
            720,
            140,
            65,
            0,
        ),
        "longsword" => retagged("landsknecht-longsword"),
        "military_pike" => retagged("kriegsspiess"),
        "rapier" => retagged("reitschwert-1540"),
        "rondel_dagger" => retagged("rondel-dagger"),
        "spear" => retagged("short-spear"),
        "war_hammer" => retagged("reiter-war-hammer"),
        "zweihander" => retagged("zweihander"),
        _ => return None,
    }
}
