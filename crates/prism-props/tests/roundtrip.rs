//! Exercises `props!` the way a downstream crate does: define types, walk
//! them, save and load them with no per-type code.

use prism_core::Id;
use prism_math::{Color, Vec3};
use prism_props::{Kind, Reflect, ReflectStatic, Subtype, Value, flags, props, serial, walk};

props! {
    /// How a viewport shades geometry.
    pub enum Shading {
        /// Flat matcap.
        Solid = 0,
        Wire = 1 => { label: "Wireframe" },
        Preview = 10 => { label: "Material Preview" },
    }
}

props! {
    /// A light source.
    pub struct Light {
        pub color: Color = Color::WHITE => { id: 1 },
        /// Radiant power.
        pub power: f64 = 10.0 => { id: 2, hard: 0.0.., soft: 0.0..=1000.0, step: 1.0 },
        pub enabled: bool = true => { id: 3 },
    }
}

props! {
    /// Settings for a 3D viewport.
    pub struct ViewportSettings {
        /// Field of view.
        pub fov: f64 = 0.8 => { id: 1, hard: 0.01..=3.1, subtype: Angle, flags: ANIMATABLE },
        pub shading: Shading = Shading::Solid => { id: 2 },
        pub name: String = "Viewport".to_owned() => { id: 3, label: "Title" },
        pub target: Vec3 = Vec3::ZERO => { id: 4, subtype: Translation },
        pub key_light: Light = Light::default() => { id: 5 },
        pub fill_lights: Vec<Light> = Vec::new() => { id: 6 },
        pub layers: Vec<i64> = vec![1, 2, 3] => { id: 7 },
        pub camera: Id = Id::NONE => { id: 8 },
        pub samples: u32 = 16 => { id: 9, hard: 1..=4096 },
        pub scratch: i64 = 0 => { id: 10, flags: SKIP_SAVE | HIDDEN },
        implicit_id: f64 = 1.5,
    }
}

#[test]
fn metadata_is_generated() {
    let info = ViewportSettings::info();
    assert_eq!(info.name, "ViewportSettings");
    assert_eq!(info.doc, "Settings for a 3D viewport.");
    let fov = info.field("fov").unwrap();
    assert_eq!(fov.id, 1);
    assert_eq!(fov.label, "Fov");
    assert_eq!(fov.doc, "Field of view.");
    assert_eq!(fov.kind, Kind::F64);
    assert_eq!(fov.default, Value::F64(0.8));
    assert_eq!(fov.hard.unwrap().max, 3.1);
    assert_eq!(fov.subtype, Subtype::Angle);
    assert!(fov.flags.contains(flags::ANIMATABLE));
    assert_eq!(info.field("name").unwrap().label, "Title");
    assert_eq!(info.field("key_light").unwrap().kind, Kind::Struct(Light::info()));
    assert_eq!(info.field("fill_lights").unwrap().kind, Kind::List(Box::new(Kind::Struct(Light::info()))));
    assert_eq!(info.field("layers").unwrap().kind, Kind::List(Box::new(Kind::I64)));
    assert_eq!(info.field("shading").unwrap().kind, Kind::Enum(Shading::info()));
    assert_eq!(info.field("samples").unwrap().kind, Kind::I64);
    assert_eq!(info.field("implicit_id").unwrap().id, 11, "index + 1");
    assert!(info.field("scratch").unwrap().is_hidden());
    assert!(!info.field("scratch").unwrap().is_saved());
    // Light's power soft range is clamped into the hard range's open end fine.
    let power = Light::info().field("power").unwrap();
    assert_eq!(power.soft.unwrap().max, 1000.0);
    assert_eq!(power.step, Some(1.0));
}

#[test]
fn enums() {
    assert_eq!(Shading::default(), Shading::Solid);
    assert_eq!(Shading::ALL.len(), 3);
    assert_eq!(Shading::Wire.label(), "Wireframe");
    assert_eq!(Shading::Solid.label(), "Solid");
    assert_eq!(Shading::from_i64(10), Some(Shading::Preview));
    assert_eq!(Shading::from_i64(2), None);
    assert_eq!(Shading::info().variants[2].value, 10);
    assert_eq!(Shading::info().name, "Shading");
}

#[test]
fn get_and_set_dynamically() {
    let mut v = ViewportSettings::default();
    let r: &mut dyn Reflect = &mut v;
    let fov = r.type_info().field_index("fov").unwrap();
    assert_eq!(r.get(fov), Value::F64(0.8));
    r.set(fov, Value::F64(1.2)).unwrap();
    assert_eq!(r.get(fov), Value::F64(1.2));
    assert!(r.set(fov, Value::Str("no".into())).is_err());
    r.set_by_name("shading", Value::Enum(1)).unwrap();
    assert_eq!(r.get_by_name("shading"), Some(Value::Enum(1)));
    assert!(r.set_by_name("shading", Value::Enum(99)).is_err(), "unknown variant");
    r.set_by_name("samples", Value::I64(64)).unwrap();
    assert!(r.set_by_name("samples", Value::I64(-1)).is_err(), "u32 rejects negatives");
    // Nested struct and lists.
    let kl = r.type_info().field_index("key_light").unwrap();
    r.get_struct_mut(kl).unwrap().set_by_name("power", Value::F64(50.0)).unwrap();
    let fl = r.type_info().field_index("fill_lights").unwrap();
    let list = r.get_list_mut(fl).unwrap();
    list.push_default();
    list.get_struct_mut(0).unwrap().set_by_name("enabled", Value::Bool(false)).unwrap();
    assert_eq!(v.fov, 1.2);
    assert_eq!(v.shading, Shading::Wire);
    assert_eq!(v.samples, 64);
    assert_eq!(v.key_light.power, 50.0);
    assert_eq!(v.fill_lights.len(), 1);
    assert!(!v.fill_lights[0].enabled);
    assert!((&v as &dyn Reflect).downcast_ref::<Light>().is_none());
    let boxed: Box<dyn Reflect> = Box::new(v.clone());
    assert_eq!(boxed.clone().downcast_ref::<ViewportSettings>(), Some(&v));
}

#[test]
fn paths_and_walk() {
    let mut v = ViewportSettings::default();
    v.fill_lights.push(Light { color: Color::RED, power: 3.0, enabled: true });
    let r: &mut dyn Reflect = &mut v;
    assert_eq!(walk::get_path(r, "key_light.power"), Some(Value::F64(10.0)));
    assert_eq!(walk::get_path(r, "fill_lights[0].color"), Some(Value::Color(Color::RED)));
    assert_eq!(walk::get_path(r, "layers[2]"), Some(Value::I64(3)));
    assert_eq!(walk::get_path(r, "layers[9]"), None);
    assert_eq!(walk::get_path(r, "key_light"), None, "not a leaf");
    assert_eq!(walk::get_path(r, "nope.x"), None);
    walk::set_path(r, "fill_lights[0].power", Value::F64(7.0)).unwrap();
    walk::set_path(r, "layers[0]", Value::I64(42)).unwrap();
    assert!(walk::set_path(r, "fill_lights[3].power", Value::F64(1.0)).is_err());
    assert!(walk::set_path(r, "bogus", Value::F64(1.0)).is_err());
    assert_eq!(v.fill_lights[0].power, 7.0);
    assert_eq!(v.layers[0], 42);

    let mut paths = Vec::new();
    walk::walk(&v, &mut |path, _field, _value| paths.push(path.to_owned()));
    assert_eq!(
        paths,
        vec![
            "fov", "shading", "name", "target",
            "key_light.color", "key_light.power", "key_light.enabled",
            "fill_lights[0].color", "fill_lights[0].power", "fill_lights[0].enabled",
            "layers[0]", "layers[1]", "layers[2]",
            "camera", "samples", "scratch", "implicit_id",
        ]
    );
    let dbg = format!("{:?}", &v as &dyn Reflect);
    assert!(dbg.starts_with("ViewportSettings {"));
    assert!(dbg.contains("fill_lights: [1 items]"));
}

#[test]
fn serialization_roundtrip() {
    let v = ViewportSettings {
        fov: 1.1,
        shading: Shading::Preview,
        name: "Left 🦊".into(),
        target: Vec3::new(1.0, -2.0, 3.5),
        key_light: Light { power: 99.0, ..Light::default() },
        fill_lights: vec![
            Light { color: Color::hex(0x336699), power: 1.0, enabled: false },
            Light { color: Color::GREEN, power: 2.0, enabled: true },
        ],
        layers: vec![7, 8],
        camera: Id(1234),
        samples: 128,
        scratch: 555,
        implicit_id: -1.0,
    };

    let bytes = serial::to_bytes(&v);
    let mut back = ViewportSettings::default();
    serial::from_bytes(&mut back, &bytes).unwrap();
    assert_eq!(back.scratch, 0, "SKIP_SAVE fields are not written");
    back.scratch = 555;
    assert_eq!(back, v);
    // Saving again is byte-stable.
    assert_eq!(serial::to_bytes(&back), bytes);
}

props! {
    /// Version 1 of a struct: fewer fields, one different type.
    pub struct SettingsV1 {
        pub speed: f64 = 1.0 => { id: 1 },
        pub mode: i64 = 0 => { id: 2 },
        pub old_only: bool = true => { id: 3 },
    }
}

props! {
    /// Version 2: `speed` renamed, `mode` changed type, `old_only` removed,
    /// `extra` added.
    pub struct SettingsV2 {
        pub velocity: f64 = 1.0 => { id: 1 },
        pub mode: String = "auto".into() => { id: 2 },
        pub extra: Vec3 = Vec3::ONE => { id: 4 },
    }
}

#[test]
fn schema_evolution() {
    let v1 = SettingsV1 { speed: 3.0, mode: 5, old_only: false };
    let mut v2 = SettingsV2::default();
    serial::from_bytes(&mut v2, &serial::to_bytes(&v1)).unwrap();
    assert_eq!(v2.velocity, 3.0, "renamed field loads by id");
    assert_eq!(v2.mode, "auto", "type change keeps the default");
    assert_eq!(v2.extra, Vec3::ONE, "new field keeps the default");

    let v2 = SettingsV2 { velocity: 9.0, mode: "x".into(), extra: Vec3::Z };
    let mut v1 = SettingsV1::default();
    serial::from_bytes(&mut v1, &serial::to_bytes(&v2)).unwrap();
    assert_eq!(v1.speed, 9.0);
    assert_eq!(v1.mode, 0, "string can't load into an int");
    assert!(v1.old_only, "unknown field in the file is skipped, removed field keeps default");

    // Truncated data is an error, not a panic.
    let bytes = serial::to_bytes(&v2);
    assert!(serial::from_bytes(&mut SettingsV2::default(), &bytes[..bytes.len() - 3]).is_err());
}
