//! `<mesh shape>`: Bevy's mesh primitives, one struct per shape. A shape's props are its Bevy
//! constructor's parameters in camelCase plus its mesh builder's settings; an unset prop is
//! Bevy's default. 2D shapes lie in the XY plane facing +Z; with `depth` they are Bevy's
//! `Extrusion`. `check` refuses every value Bevy would panic or error on, so `mesh` never panics.

use bevy::math::{Dir3, Vec2};
use bevy::mesh::{
    CapsuleUvProfile, CircularMeshUvMode, ConeAnchor, CylinderAnchor, Extrudable, Mesh, MeshBuilder, Meshable,
};
use bevy::shape::{
    Annulus, Arc2d, Capsule2d, Capsule3d, Circle, CircularSector, CircularSegment, Cone, ConicalFrustum, ConvexPolygon,
    Cuboid, Cylinder, Ellipse, Extrusion, Plane3d, Primitive2d, Rectangle, RegularPolygon, Rhombus, Sphere,
    Tetrahedron, Torus, Triangle2d, Triangle3d,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::f32::consts::{PI, TAU};

type V2 = [f32; 2];
type V3 = [f32; 3];

/// A bad prop: its name and what is wrong with it.
type Bad = (&'static str, String);

/// Every shape `<mesh>` builds, tagged by `shape`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum MeshShape {
    Cuboid(CuboidMesh),
    Sphere(SphereMesh),
    Plane(PlaneMesh),
    Cylinder(CylinderMesh),
    Capsule3d(Capsule3dMesh),
    Cone(ConeMesh),
    ConicalFrustum(ConicalFrustumMesh),
    Torus(TorusMesh),
    Tetrahedron(TetrahedronMesh),
    Triangle3d(Triangle3dMesh),
    Circle(CircleMesh),
    CircularSector(CircularSectorMesh),
    CircularSegment(CircularSegmentMesh),
    Ellipse(EllipseMesh),
    Annulus(AnnulusMesh),
    Rectangle(RectangleMesh),
    Rhombus(RhombusMesh),
    RegularPolygon(RegularPolygonMesh),
    Triangle2d(Triangle2dMesh),
    Capsule2d(Capsule2dMesh),
    ConvexPolygon(ConvexPolygonMesh),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SphereKind {
    Ico,
    Uv,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CylinderAnchorProp {
    MidPoint,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConeAnchorProp {
    MidPoint,
    Tip,
    Base,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UvProfile {
    Aspect,
    Uniform,
    Fixed,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CuboidMesh {
    pub size: Option<V3>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SphereMesh {
    pub radius: Option<f32>,
    pub kind: Option<SphereKind>,
    pub subdivisions: Option<u32>,
    pub sectors: Option<u32>,
    pub stacks: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaneMesh {
    pub size: Option<V2>,
    pub normal: Option<V3>,
    pub subdivisions: Option<u32>,
    pub subdivisions_x: Option<u32>,
    pub subdivisions_z: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CylinderMesh {
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub resolution: Option<u32>,
    pub segments: Option<u32>,
    pub caps: Option<bool>,
    pub anchor: Option<CylinderAnchorProp>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capsule3dMesh {
    pub radius: Option<f32>,
    pub length: Option<f32>,
    pub rings: Option<u32>,
    pub longitudes: Option<u32>,
    pub latitudes: Option<u32>,
    pub uv_profile: Option<UvProfile>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConeMesh {
    pub radius: Option<f32>,
    pub height: Option<f32>,
    pub resolution: Option<u32>,
    pub anchor: Option<ConeAnchorProp>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConicalFrustumMesh {
    pub radius_top: Option<f32>,
    pub radius_bottom: Option<f32>,
    pub height: Option<f32>,
    pub resolution: Option<u32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TorusMesh {
    pub inner_radius: Option<f32>,
    pub outer_radius: Option<f32>,
    pub minor_resolution: Option<u32>,
    pub major_resolution: Option<u32>,
    pub angle_range: Option<V2>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TetrahedronMesh {
    pub vertices: Option<[V3; 4]>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Triangle3dMesh {
    pub vertices: Option<[V3; 3]>,
}

// The 2D shapes: each takes `depth` (an extrusion) and, with it, `segments`.

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CircleMesh {
    pub radius: Option<f32>,
    pub resolution: Option<u32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CircularSectorMesh {
    pub radius: Option<f32>,
    pub angle: Option<f32>,
    pub resolution: Option<u32>,
    pub uv_angle: Option<f32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CircularSegmentMesh {
    pub radius: Option<f32>,
    pub half_angle: Option<f32>,
    pub resolution: Option<u32>,
    pub uv_angle: Option<f32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EllipseMesh {
    pub half_size: Option<V2>,
    pub resolution: Option<u32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnulusMesh {
    pub inner_radius: Option<f32>,
    pub outer_radius: Option<f32>,
    pub resolution: Option<u32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RectangleMesh {
    pub size: Option<V2>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RhombusMesh {
    pub horizontal_diagonal: Option<f32>,
    pub vertical_diagonal: Option<f32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegularPolygonMesh {
    pub circumradius: Option<f32>,
    pub sides: Option<u32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Triangle2dMesh {
    pub vertices: Option<[V2; 3]>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capsule2dMesh {
    pub radius: Option<f32>,
    pub length: Option<f32>,
    pub resolution: Option<u32>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConvexPolygonMesh {
    /// Required; in either winding order.
    pub vertices: Option<Vec<V2>>,
    pub depth: Option<f32>,
    pub segments: Option<u32>,
}

impl MeshShape {
    /// Parses the props of a `<mesh>` other than `color`. An error names the shape and the prop.
    pub fn from_props(props: Map<String, Value>) -> Result<Self, String> {
        let e = match serde_json::from_value(Value::Object(props.clone())) {
            Ok(s) => return Ok(s),
            Err(e) => e,
        };
        // Find the prop at fault: parse the shape with each prop alone.
        let Some(Value::String(shape)) = props.get("shape") else { return Err(format!("shape: {e}")) };
        let alone = |prop: Option<(&String, &Value)>| {
            let mut m = Map::new();
            m.insert("shape".into(), Value::String(shape.clone()));
            m.extend(prop.map(|(k, v)| (k.clone(), v.clone())));
            serde_json::from_value::<MeshShape>(Value::Object(m)).err()
        };
        if let Some(e) = alone(None) {
            return Err(format!("shape: {e}"));
        }
        for (k, v) in props.iter().filter(|(k, _)| *k != "shape") {
            if let Some(e) = alone(Some((k, v))) {
                return Err(format!("{shape}: {k}: {e}"));
            }
        }
        Err(format!("{shape}: {e}"))
    }

    /// The `shape` prop: Bevy's type name in camelCase.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Cuboid(_) => "cuboid",
            Self::Sphere(_) => "sphere",
            Self::Plane(_) => "plane",
            Self::Cylinder(_) => "cylinder",
            Self::Capsule3d(_) => "capsule3d",
            Self::Cone(_) => "cone",
            Self::ConicalFrustum(_) => "conicalFrustum",
            Self::Torus(_) => "torus",
            Self::Tetrahedron(_) => "tetrahedron",
            Self::Triangle3d(_) => "triangle3d",
            Self::Circle(_) => "circle",
            Self::CircularSector(_) => "circularSector",
            Self::CircularSegment(_) => "circularSegment",
            Self::Ellipse(_) => "ellipse",
            Self::Annulus(_) => "annulus",
            Self::Rectangle(_) => "rectangle",
            Self::Rhombus(_) => "rhombus",
            Self::RegularPolygon(_) => "regularPolygon",
            Self::Triangle2d(_) => "triangle2d",
            Self::Capsule2d(_) => "capsule2d",
            Self::ConvexPolygon(_) => "convexPolygon",
        }
    }

    /// Refuses every value Bevy would panic or error on; the error names the shape and the prop.
    pub fn check(&self) -> Result<(), String> {
        let r = match self {
            Self::Cuboid(s) => s.check(),
            Self::Sphere(s) => s.check(),
            Self::Plane(s) => s.check(),
            Self::Cylinder(s) => s.check(),
            Self::Capsule3d(s) => s.check(),
            Self::Cone(s) => s.check(),
            Self::ConicalFrustum(s) => s.check(),
            Self::Torus(s) => s.check(),
            Self::Tetrahedron(s) => finite("vertices", s.vertices.iter().flatten().flatten()),
            Self::Triangle3d(s) => finite("vertices", s.vertices.iter().flatten().flatten()),
            Self::Circle(s) => positive("radius", s.radius)
                .and(at_least("resolution", s.resolution, 3))
                .and(extrude(s.depth, s.segments)),
            Self::CircularSector(s) => s.check(),
            Self::CircularSegment(s) => s.check(),
            Self::Ellipse(s) => positive_all("halfSize", s.half_size.iter().flatten())
                .and(at_least("resolution", s.resolution, 3))
                .and(extrude(s.depth, s.segments)),
            Self::Annulus(s) => s.check(),
            Self::Rectangle(s) => positive_all("size", s.size.iter().flatten()).and(extrude(s.depth, s.segments)),
            Self::Rhombus(s) => positive("horizontalDiagonal", s.horizontal_diagonal)
                .and(positive("verticalDiagonal", s.vertical_diagonal))
                .and(extrude(s.depth, s.segments)),
            Self::RegularPolygon(s) => positive("circumradius", s.circumradius)
                .and(at_least("sides", s.sides, 3))
                .and(extrude(s.depth, s.segments)),
            Self::Triangle2d(s) => {
                finite("vertices", s.vertices.iter().flatten().flatten()).and(extrude(s.depth, s.segments))
            }
            Self::Capsule2d(s) => positive("radius", s.radius)
                .and(positive("length", s.length))
                .and(at_least("resolution", s.resolution, 3))
                .and(extrude(s.depth, s.segments)),
            Self::ConvexPolygon(s) => s.check(),
        };
        r.map_err(|(prop, m)| format!("{}: {prop}: {m}", self.name()))
    }

    /// Builds the mesh through Bevy's builder. Call `check` first.
    pub fn mesh(&self) -> Mesh {
        match self {
            Self::Cuboid(s) => {
                let [x, y, z] = s.size.unwrap_or([1.0; 3]);
                Cuboid::new(x, y, z).mesh().build()
            }
            Self::Sphere(s) => s.mesh(),
            Self::Plane(s) => s.mesh(),
            Self::Cylinder(s) => s.mesh(),
            Self::Capsule3d(s) => s.mesh(),
            Self::Cone(s) => s.mesh(),
            Self::ConicalFrustum(s) => s.mesh(),
            Self::Torus(s) => s.mesh(),
            Self::Tetrahedron(s) => {
                let t = s.vertices.map_or_else(Tetrahedron::default, |[a, b, c, d]| {
                    Tetrahedron::new(a.into(), b.into(), c.into(), d.into())
                });
                t.mesh().build()
            }
            Self::Triangle3d(s) => {
                let t = s
                    .vertices
                    .map_or_else(Triangle3d::default, |[a, b, c]| Triangle3d::new(a.into(), b.into(), c.into()));
                t.mesh().build()
            }
            Self::Circle(s) => {
                let c = Circle::new(s.radius.unwrap_or(Circle::default().radius));
                flat(c, s.depth, s.segments, |b| match s.resolution {
                    Some(r) => b.resolution(r),
                    None => b,
                })
            }
            Self::CircularSector(s) => {
                let d = Arc2d::default();
                let c = CircularSector::from_radians(s.radius.unwrap_or(d.radius), s.angle.unwrap_or(2.0 * d.half_angle));
                flat(c, s.depth, s.segments, |mut b| {
                    if let Some(r) = s.resolution {
                        b = b.resolution(r);
                    }
                    if let Some(a) = s.uv_angle {
                        b = b.uv_mode(CircularMeshUvMode::Mask { angle: a });
                    }
                    b
                })
            }
            Self::CircularSegment(s) => {
                let d = Arc2d::default();
                let c = CircularSegment::new(s.radius.unwrap_or(d.radius), s.half_angle.unwrap_or(d.half_angle));
                flat(c, s.depth, s.segments, |mut b| {
                    if let Some(r) = s.resolution {
                        b = b.resolution(r);
                    }
                    if let Some(a) = s.uv_angle {
                        b = b.uv_mode(CircularMeshUvMode::Mask { angle: a });
                    }
                    b
                })
            }
            Self::Ellipse(s) => {
                let e = s.half_size.map_or_else(Ellipse::default, |[x, y]| Ellipse::new(x, y));
                flat(e, s.depth, s.segments, |b| match s.resolution {
                    Some(r) => b.resolution(r),
                    None => b,
                })
            }
            Self::Annulus(s) => {
                let d = Annulus::default();
                let a = Annulus::new(
                    s.inner_radius.unwrap_or(d.inner_circle.radius),
                    s.outer_radius.unwrap_or(d.outer_circle.radius),
                );
                flat(a, s.depth, s.segments, |b| match s.resolution {
                    Some(r) => b.resolution(r),
                    None => b,
                })
            }
            Self::Rectangle(s) => {
                let [w, h] = s.size.unwrap_or([1.0; 2]);
                flat(Rectangle::new(w, h), s.depth, s.segments, |b| b)
            }
            Self::Rhombus(s) => {
                let r = Rhombus::new(s.horizontal_diagonal.unwrap_or(1.0), s.vertical_diagonal.unwrap_or(1.0));
                flat(r, s.depth, s.segments, |b| b)
            }
            Self::RegularPolygon(s) => {
                let d = RegularPolygon::default();
                let p = RegularPolygon::new(s.circumradius.unwrap_or(d.circumcircle.radius), s.sides.unwrap_or(d.sides));
                flat(p, s.depth, s.segments, |b| b)
            }
            Self::Triangle2d(s) => {
                let t = s
                    .vertices
                    .map_or_else(Triangle2d::default, |[a, b, c]| Triangle2d::new(a.into(), b.into(), c.into()));
                flat(t, s.depth, s.segments, |b| b)
            }
            Self::Capsule2d(s) => {
                let d = Capsule2d::default();
                let c = Capsule2d::new(s.radius.unwrap_or(d.radius), s.length.unwrap_or(2.0 * d.half_length));
                flat(c, s.depth, s.segments, |b| match s.resolution {
                    Some(r) => b.resolution(r),
                    None => b,
                })
            }
            Self::ConvexPolygon(s) => {
                let mut vs: Vec<Vec2> = s.vertices.iter().flatten().map(|&v| v.into()).collect();
                // Bevy's mesh faces +Z only for counterclockwise vertices.
                if signed_area(&vs) < 0.0 {
                    vs.reverse();
                }
                flat(ConvexPolygon::new(vs).expect("checked"), s.depth, s.segments, |b| b)
            }
        }
    }
}

/// A 2D shape's mesh, or with `depth` its extrusion along Z. `f` sets the base builder.
fn flat<P>(shape: P, depth: Option<f32>, segments: Option<u32>, f: impl Fn(P::Output) -> P::Output) -> Mesh
where
    P: Primitive2d + Meshable,
    P::Output: Extrudable,
{
    let Some(depth) = depth else { return f(shape.mesh()).build() };
    let mut b = Extrusion::new(shape, depth).mesh().with_inner(f);
    if let Some(n) = segments {
        b = b.segments(n as usize);
    }
    b.build()
}

impl SphereMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radius", self.radius)?;
        let uv = self.kind == Some(SphereKind::Uv);
        if uv && self.subdivisions.is_some() {
            return Err(("subdivisions", "is for kind=\"ico\"; a uv sphere takes sectors and stacks".into()));
        }
        if !uv {
            for (prop, v) in [("sectors", self.sectors), ("stacks", self.stacks)] {
                if v.is_some() {
                    return Err((prop, "is for kind=\"uv\"".into()));
                }
            }
        }
        if let Some(n) = self.subdivisions.filter(|&n| n > 79) {
            return Err(("subdivisions", format!("must be at most 79 (Bevy's icosphere limit), not {n}")));
        }
        at_least("sectors", self.sectors, 3)?;
        at_least("stacks", self.stacks, 2)
    }

    fn mesh(&self) -> Mesh {
        let b = Sphere::new(self.radius.unwrap_or(0.5)).mesh();
        match self.kind {
            // Bevy has no default for a uv sphere; these are the ones its docs suggest.
            Some(SphereKind::Uv) => b.uv(self.sectors.unwrap_or(32), self.stacks.unwrap_or(18)),
            _ => b.ico(self.subdivisions.unwrap_or(5)).expect("checked"),
        }
    }
}

impl CuboidMesh {
    fn check(&self) -> Result<(), Bad> {
        positive_all("size", self.size.iter().flatten())
    }
}

impl PlaneMesh {
    fn check(&self) -> Result<(), Bad> {
        positive_all("size", self.size.iter().flatten())?;
        if let Some(n) = self.normal
            && Dir3::new(n.into()).is_err()
        {
            return Err(("normal", format!("must be a nonzero finite vector, not {n:?}")));
        }
        Ok(())
    }

    fn mesh(&self) -> Mesh {
        let [x, z] = self.size.unwrap_or([1.0; 2]);
        let mut b = Plane3d::default().mesh().size(x, z);
        if let Some(n) = self.normal {
            b = b.normal(Dir3::new(n.into()).expect("checked"));
        }
        if let Some(n) = self.subdivisions {
            b = b.subdivisions(n);
        }
        if let Some(n) = self.subdivisions_x {
            b = b.subdivisions_x(n);
        }
        if let Some(n) = self.subdivisions_z {
            b = b.subdivisions_z(n);
        }
        b.build()
    }
}

impl CylinderMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radius", self.radius)?;
        positive("height", self.height)?;
        at_least("resolution", self.resolution, 3)?;
        at_least("segments", self.segments, 1)
    }

    fn mesh(&self) -> Mesh {
        let d = Cylinder::default();
        let c = Cylinder::new(self.radius.unwrap_or(d.radius), self.height.unwrap_or(2.0 * d.half_height));
        let mut b = c.mesh();
        if let Some(n) = self.resolution {
            b = b.resolution(n);
        }
        if let Some(n) = self.segments {
            b = b.segments(n);
        }
        if self.caps == Some(false) {
            b = b.without_caps();
        }
        if let Some(a) = self.anchor {
            b = b.anchor(match a {
                CylinderAnchorProp::MidPoint => CylinderAnchor::MidPoint,
                CylinderAnchorProp::Top => CylinderAnchor::Top,
                CylinderAnchorProp::Bottom => CylinderAnchor::Bottom,
            });
        }
        b.build()
    }
}

impl Capsule3dMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radius", self.radius)?;
        positive("length", self.length)?;
        at_least("longitudes", self.longitudes, 3)?;
        match self.latitudes {
            Some(n) if n < 4 || n % 2 != 0 => Err(("latitudes", format!("must be even and at least 4, not {n}"))),
            _ => Ok(()),
        }
    }

    fn mesh(&self) -> Mesh {
        let d = Capsule3d::default();
        let mut b = Capsule3d::new(self.radius.unwrap_or(d.radius), self.length.unwrap_or(2.0 * d.half_length)).mesh();
        if let Some(n) = self.rings {
            b = b.rings(n);
        }
        if let Some(n) = self.longitudes {
            b = b.longitudes(n);
        }
        if let Some(n) = self.latitudes {
            b = b.latitudes(n);
        }
        if let Some(p) = self.uv_profile {
            b = b.uv_profile(match p {
                UvProfile::Aspect => CapsuleUvProfile::Aspect,
                UvProfile::Uniform => CapsuleUvProfile::Uniform,
                UvProfile::Fixed => CapsuleUvProfile::Fixed,
            });
        }
        b.build()
    }
}

impl ConeMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radius", self.radius)?;
        positive("height", self.height)?;
        at_least("resolution", self.resolution, 3)
    }

    fn mesh(&self) -> Mesh {
        let d = Cone::default();
        let mut b = Cone::new(self.radius.unwrap_or(d.radius), self.height.unwrap_or(d.height)).mesh();
        if let Some(n) = self.resolution {
            b = b.resolution(n);
        }
        if let Some(a) = self.anchor {
            b = b.anchor(match a {
                ConeAnchorProp::MidPoint => ConeAnchor::MidPoint,
                ConeAnchorProp::Tip => ConeAnchor::Tip,
                ConeAnchorProp::Base => ConeAnchor::Base,
            });
        }
        b.build()
    }
}

impl ConicalFrustumMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radiusTop", self.radius_top)?;
        positive("radiusBottom", self.radius_bottom)?;
        positive("height", self.height)?;
        at_least("resolution", self.resolution, 3)?;
        at_least("segments", self.segments, 1)
    }

    fn mesh(&self) -> Mesh {
        let d = ConicalFrustum::default();
        let f = ConicalFrustum {
            radius_top: self.radius_top.unwrap_or(d.radius_top),
            radius_bottom: self.radius_bottom.unwrap_or(d.radius_bottom),
            height: self.height.unwrap_or(d.height),
        };
        let mut b = f.mesh();
        if let Some(n) = self.resolution {
            b = b.resolution(n);
        }
        if let Some(n) = self.segments {
            b = b.segments(n);
        }
        b.build()
    }
}

impl TorusMesh {
    /// Bevy's default torus: inner radius 0.5, outer 1.
    fn radii(&self) -> (f32, f32) {
        let d = Torus::default();
        (
            self.inner_radius.unwrap_or(d.major_radius - d.minor_radius),
            self.outer_radius.unwrap_or(d.major_radius + d.minor_radius),
        )
    }

    fn check(&self) -> Result<(), Bad> {
        positive("innerRadius", self.inner_radius)?;
        positive("outerRadius", self.outer_radius)?;
        let (inner, outer) = self.radii();
        if inner >= outer {
            let prop = if self.inner_radius.is_some() { "innerRadius" } else { "outerRadius" };
            return Err((prop, format!("the inner radius ({inner}) must be below the outer one ({outer})")));
        }
        at_least("minorResolution", self.minor_resolution, 3)?;
        at_least("majorResolution", self.major_resolution, 3)?;
        if let Some([a, b]) = self.angle_range {
            finite("angleRange", [a, b].iter())?;
            if a >= b {
                return Err(("angleRange", format!("must go up, not [{a}, {b}]")));
            }
        }
        Ok(())
    }

    fn mesh(&self) -> Mesh {
        let (inner, outer) = self.radii();
        let mut b = Torus::new(inner, outer).mesh();
        if let Some(n) = self.minor_resolution {
            b = b.minor_resolution(n as usize);
        }
        if let Some(n) = self.major_resolution {
            b = b.major_resolution(n as usize);
        }
        if let Some([a, e]) = self.angle_range {
            b = b.angle_range(a..=e);
        }
        b.build()
    }
}

impl CircularSectorMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radius", self.radius)?;
        in_range("angle", self.angle, TAU, "2π")?;
        at_least("resolution", self.resolution, 3)?;
        finite("uvAngle", self.uv_angle.iter())?;
        extrude(self.depth, self.segments)
    }
}

impl CircularSegmentMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("radius", self.radius)?;
        in_range("halfAngle", self.half_angle, PI, "π")?;
        at_least("resolution", self.resolution, 3)?;
        finite("uvAngle", self.uv_angle.iter())?;
        extrude(self.depth, self.segments)
    }
}

impl AnnulusMesh {
    fn check(&self) -> Result<(), Bad> {
        positive("innerRadius", self.inner_radius)?;
        positive("outerRadius", self.outer_radius)?;
        let d = Annulus::default();
        let inner = self.inner_radius.unwrap_or(d.inner_circle.radius);
        let outer = self.outer_radius.unwrap_or(d.outer_circle.radius);
        if inner >= outer {
            let prop = if self.inner_radius.is_some() { "innerRadius" } else { "outerRadius" };
            return Err((prop, format!("the inner radius ({inner}) must be below the outer one ({outer})")));
        }
        at_least("resolution", self.resolution, 3)?;
        extrude(self.depth, self.segments)
    }
}

impl ConvexPolygonMesh {
    fn check(&self) -> Result<(), Bad> {
        let Some(vs) = &self.vertices else { return Err(("vertices", "is required".into())) };
        if vs.len() < 3 {
            return Err(("vertices", format!("must have at least 3 points, not {}", vs.len())));
        }
        finite("vertices", vs.iter().flatten())?;
        let vs: Vec<Vec2> = vs.iter().map(|&v| v.into()).collect();
        if signed_area(&vs).abs() <= f32::EPSILON {
            return Err(("vertices", "have no area".into()));
        }
        if ConvexPolygon::new(vs).is_err() {
            return Err(("vertices", "are not convex".into()));
        }
        extrude(self.depth, self.segments)
    }
}

/// Twice the polygon's area, positive when counterclockwise.
fn signed_area(vs: &[Vec2]) -> f32 {
    (0..vs.len()).map(|i| vs[i].perp_dot(vs[(i + 1) % vs.len()])).sum()
}

fn positive(prop: &'static str, v: Option<f32>) -> Result<(), Bad> {
    match v {
        Some(x) if !(x.is_finite() && x > 0.0) => Err((prop, format!("must be positive and finite, not {x}"))),
        _ => Ok(()),
    }
}

fn positive_all<'a>(prop: &'static str, mut vs: impl Iterator<Item = &'a f32>) -> Result<(), Bad> {
    vs.try_for_each(|&x| positive(prop, Some(x)))
}

fn finite<'a>(prop: &'static str, mut vs: impl Iterator<Item = &'a f32>) -> Result<(), Bad> {
    match vs.find(|x| !x.is_finite()) {
        Some(x) => Err((prop, format!("must be finite, not {x}"))),
        None => Ok(()),
    }
}

fn at_least(prop: &'static str, v: Option<u32>, min: u32) -> Result<(), Bad> {
    match v {
        Some(n) if n < min => Err((prop, format!("must be at least {min}, not {n}"))),
        _ => Ok(()),
    }
}

/// An angle in (0, max].
fn in_range(prop: &'static str, v: Option<f32>, max: f32, max_name: &str) -> Result<(), Bad> {
    positive(prop, v)?;
    match v {
        Some(x) if x > max => Err((prop, format!("must be at most {max_name} ({max}), not {x}"))),
        _ => Ok(()),
    }
}

/// `depth` and `segments`, the extrusion of a 2D shape.
fn extrude(depth: Option<f32>, segments: Option<u32>) -> Result<(), Bad> {
    positive("depth", depth)?;
    if segments.is_some() && depth.is_none() {
        return Err(("segments", "needs depth (segments are along an extrusion)".into()));
    }
    at_least("segments", segments, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<MeshShape, String> {
        let Value::Object(m) = v else { unreachable!() };
        let s = MeshShape::from_props(m)?;
        s.check()?;
        Ok(s)
    }

    fn positions(m: &Mesh) -> Vec<[f32; 3]> {
        m.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec()
    }

    fn same(a: &Mesh, b: &Mesh) {
        assert_eq!(positions(a), positions(b));
        assert_eq!(a.indices().map(|i| i.iter().collect::<Vec<_>>()), b.indices().map(|i| i.iter().collect::<Vec<_>>()));
    }

    #[test]
    fn the_first_three_shapes_build_as_before() {
        let m = |v| parse(v).unwrap().mesh();
        same(&m(json!({ "shape": "cuboid" })), &Cuboid::new(1.0, 1.0, 1.0).into());
        same(&m(json!({ "shape": "cuboid", "size": [1, 2, 3] })), &Cuboid::new(1.0, 2.0, 3.0).into());
        same(&m(json!({ "shape": "sphere" })), &Sphere::new(0.5).into());
        same(&m(json!({ "shape": "sphere", "radius": 2 })), &Sphere::new(2.0).into());
        same(&m(json!({ "shape": "plane" })), &Plane3d::default().mesh().size(1.0, 1.0).into());
        same(&m(json!({ "shape": "plane", "size": [4, 4] })), &Plane3d::default().mesh().size(4.0, 4.0).into());
    }

    /// Each bad value fails naming the shape and the prop.
    #[test]
    fn bad_props_name_the_shape_and_the_prop() {
        let cases = [
            (json!({ "shape": "sphere", "sides": 6 }), "sphere: sides:"),
            (json!({ "shape": "cuboid", "size": [1, 2] }), "cuboid: size:"),
            (json!({ "shape": "plane", "size": [1, 2, 3] }), "plane: size:"),
            (json!({ "shape": "regularPolygon", "sides": "six" }), "regularPolygon: sides:"),
            (json!({ "shape": "circle", "resolution": -3 }), "circle: resolution:"),
            (json!({ "shape": "regularPolygon", "sides": 2 }), "regularPolygon: sides:"),
            (json!({ "shape": "circle", "resolution": 2 }), "circle: resolution:"),
            (json!({ "shape": "circularSector", "resolution": 1 }), "circularSector: resolution:"),
            (json!({ "shape": "capsule2d", "resolution": 2 }), "capsule2d: resolution:"),
            (json!({ "shape": "cylinder", "resolution": 2 }), "cylinder: resolution:"),
            (json!({ "shape": "cylinder", "segments": 0 }), "cylinder: segments:"),
            (json!({ "shape": "cone", "resolution": 0 }), "cone: resolution:"),
            (json!({ "shape": "conicalFrustum", "segments": 0 }), "conicalFrustum: segments:"),
            (json!({ "shape": "torus", "minorResolution": 2 }), "torus: minorResolution:"),
            (json!({ "shape": "torus", "innerRadius": 1, "outerRadius": 1 }), "torus: innerRadius:"),
            (json!({ "shape": "torus", "angleRange": [1, 0] }), "torus: angleRange:"),
            (json!({ "shape": "sphere", "subdivisions": 80 }), "sphere: subdivisions:"),
            (json!({ "shape": "sphere", "kind": "uv", "subdivisions": 3 }), "sphere: subdivisions:"),
            (json!({ "shape": "sphere", "sectors": 8 }), "sphere: sectors:"),
            (json!({ "shape": "sphere", "kind": "uv", "stacks": 1 }), "sphere: stacks:"),
            (json!({ "shape": "capsule3d", "latitudes": 5 }), "capsule3d: latitudes:"),
            (json!({ "shape": "capsule3d", "longitudes": 2 }), "capsule3d: longitudes:"),
            (json!({ "shape": "annulus", "innerRadius": 2 }), "annulus: innerRadius:"),
            (json!({ "shape": "annulus", "outerRadius": 0.25 }), "annulus: outerRadius:"),
            (json!({ "shape": "circle", "segments": 2 }), "circle: segments:"),
            (json!({ "shape": "circle", "depth": 1, "segments": 0 }), "circle: segments:"),
            (json!({ "shape": "rectangle", "depth": 0 }), "rectangle: depth:"),
            (json!({ "shape": "circle", "radius": 0 }), "circle: radius:"),
            (json!({ "shape": "sphere", "radius": -1 }), "sphere: radius:"),
            (json!({ "shape": "cuboid", "size": [1, 0, 1] }), "cuboid: size:"),
            (json!({ "shape": "ellipse", "halfSize": [1, -1] }), "ellipse: halfSize:"),
            (json!({ "shape": "circularSector", "angle": 7 }), "circularSector: angle:"),
            (json!({ "shape": "circularSegment", "halfAngle": 4 }), "circularSegment: halfAngle:"),
            (json!({ "shape": "plane", "normal": [0, 0, 0] }), "plane: normal:"),
            (json!({ "shape": "convexPolygon" }), "convexPolygon: vertices:"),
            (json!({ "shape": "convexPolygon", "vertices": [[0, 0], [1, 0]] }), "convexPolygon: vertices:"),
            (json!({ "shape": "convexPolygon", "vertices": [[0, 0], [1, 0], [2, 0]] }), "convexPolygon: vertices:"),
            (json!({ "shape": "convexPolygon", "vertices": [[0, 0], [2, 0], [1, 0.5], [2, 2], [0, 2]] }), "convexPolygon: vertices:"),
            (json!({ "shape": "tetrahedron", "vertices": [[0, 0, 0], [1, 0, 0], [0, 1, 0]] }), "tetrahedron: vertices:"),
            (json!({ "shape": "dodecahedron" }), "shape:"),
            (json!({ "size": [1, 1, 1] }), "shape:"),
        ];
        for (props, want) in cases {
            let e = parse(props.clone()).expect_err(&props.to_string());
            assert!(e.starts_with(want), "{props}: {e}");
        }
    }

    /// `angle` is the whole angle: the mesh spans `angle` about its centre, and the default 4π/3.
    #[test]
    fn a_sector_spans_its_whole_angle() {
        let span = |v| {
            let ps = positions(&parse(v).unwrap().mesh());
            let angles: Vec<f32> =
                ps.iter().filter(|p| p[0].hypot(p[1]) > 1e-3).map(|p| p[0].atan2(p[1])).collect();
            let hi = angles.iter().cloned().fold(f32::MIN, f32::max);
            let lo = angles.iter().cloned().fold(f32::MAX, f32::min);
            hi - lo
        };
        let near = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        near(span(json!({ "shape": "circularSector", "angle": PI / 2.0 })), PI / 2.0);
        near(span(json!({ "shape": "circularSector" })), 4.0 * PI / 3.0);
    }

    /// The smallest settings that pass the check build without a panic.
    #[test]
    fn the_limits_build() {
        for props in [
            json!({ "shape": "regularPolygon", "sides": 3, "depth": 1, "segments": 1 }),
            json!({ "shape": "circle", "resolution": 3 }),
            json!({ "shape": "circularSector", "resolution": 3, "angle": TAU, "depth": 1 }),
            json!({ "shape": "circularSegment", "resolution": 3, "halfAngle": PI, "depth": 1 }),
            json!({ "shape": "capsule2d", "resolution": 3, "depth": 1 }),
            json!({ "shape": "capsule3d", "latitudes": 4, "longitudes": 3 }),
            json!({ "shape": "sphere", "kind": "uv", "sectors": 3, "stacks": 2 }),
            json!({ "shape": "sphere", "subdivisions": 0 }),
            json!({ "shape": "cylinder", "resolution": 3, "segments": 1 }),
            json!({ "shape": "cone", "resolution": 3 }),
            json!({ "shape": "conicalFrustum", "resolution": 3 }),
            json!({ "shape": "torus", "minorResolution": 3, "majorResolution": 3, "angleRange": [0, 0.1] }),
            json!({ "shape": "annulus", "resolution": 3, "depth": 1 }),
            json!({ "shape": "convexPolygon", "vertices": [[0, 0], [0, 1], [1, 0]], "depth": 1 }),
            json!({ "shape": "plane", "subdivisionsX": 3, "subdivisionsZ": 0 }),
        ] {
            let m = parse(props.clone()).unwrap_or_else(|e| panic!("{props}: {e}")).mesh();
            assert!(m.count_vertices() > 0, "{props}");
        }
    }
}
