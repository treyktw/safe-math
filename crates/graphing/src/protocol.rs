use engine_math::{Equation, Expression};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphDimension {
    #[serde(rename = "2d")]
    TwoD,
    #[serde(rename = "3d")]
    ThreeD,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    pub const fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisScale {
    #[default]
    Linear,
    Logarithmic,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxisSpec {
    pub label: Option<String>,
    pub unit: Option<String>,
    #[serde(default)]
    pub scale: AxisScale,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphAxes {
    #[serde(default)]
    pub x: AxisSpec,
    #[serde(default)]
    pub y: AxisSpec,
    pub z: Option<AxisSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub x: Range,
    pub y: Range,
    pub z: Option<Range>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphInteraction {
    pub pan: bool,
    pub zoom: bool,
    pub inspect: bool,
    pub legend: bool,
    pub reset: bool,
}

impl Default for GraphInteraction {
    fn default() -> Self {
        Self {
            pan: true,
            zoom: true,
            inspect: true,
            legend: true,
            reset: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStyle {
    pub color: Option<String>,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub dashed: bool,
}

fn default_width() -> f32 {
    2.0
}

fn default_opacity() -> f32 {
    1.0
}

impl Default for LineStyle {
    fn default() -> Self {
        Self {
            color: None,
            width: default_width(),
            opacity: default_opacity(),
            dashed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphSeries {
    Function2d {
        id: String,
        label: Option<String>,
        expression: String,
        #[serde(default = "default_x")]
        variable: String,
        domain: Range,
        #[serde(default)]
        style: LineStyle,
    },
    Parametric2d {
        id: String,
        label: Option<String>,
        x: String,
        y: String,
        #[serde(default = "default_t")]
        parameter: String,
        domain: Range,
        #[serde(default)]
        style: LineStyle,
    },
    Polar2d {
        id: String,
        label: Option<String>,
        radius: String,
        #[serde(default = "default_theta")]
        angle: String,
        domain: Range,
        #[serde(default)]
        style: LineStyle,
    },
    Implicit2d {
        id: String,
        label: Option<String>,
        equation: String,
        x_domain: Range,
        y_domain: Range,
        #[serde(default)]
        style: LineStyle,
    },
    Scatter2d {
        id: String,
        label: Option<String>,
        points: Vec<Point2>,
        #[serde(default)]
        style: LineStyle,
    },
    Surface3d {
        id: String,
        label: Option<String>,
        z: String,
        x_domain: Range,
        y_domain: Range,
        #[serde(default)]
        style: LineStyle,
    },
    Parametric3d {
        id: String,
        label: Option<String>,
        x: String,
        y: String,
        z: String,
        #[serde(default = "default_u")]
        u: String,
        #[serde(default = "default_v")]
        v: String,
        u_domain: Range,
        v_domain: Range,
        #[serde(default)]
        style: LineStyle,
    },
    Scatter3d {
        id: String,
        label: Option<String>,
        points: Vec<Point3>,
        #[serde(default)]
        style: LineStyle,
    },
}

fn default_x() -> String {
    "x".into()
}

fn default_t() -> String {
    "t".into()
}

fn default_theta() -> String {
    "theta".into()
}

fn default_u() -> String {
    "u".into()
}

fn default_v() -> String {
    "v".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRequest {
    pub title: Option<String>,
    pub series: Vec<GraphSeries>,
    pub axes: Option<GraphAxes>,
    pub viewport: Option<Viewport>,
    #[serde(default)]
    pub interaction: GraphInteraction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphSeriesSpec {
    Function2d {
        id: String,
        label: Option<String>,
        expression: Expression,
        variable: String,
        domain: Range,
        style: LineStyle,
    },
    Parametric2d {
        id: String,
        label: Option<String>,
        x: Expression,
        y: Expression,
        parameter: String,
        domain: Range,
        style: LineStyle,
    },
    Polar2d {
        id: String,
        label: Option<String>,
        radius: Expression,
        angle: String,
        domain: Range,
        style: LineStyle,
    },
    Implicit2d {
        id: String,
        label: Option<String>,
        equation: Equation,
        x_domain: Range,
        y_domain: Range,
        style: LineStyle,
    },
    Scatter2d {
        id: String,
        label: Option<String>,
        points: Vec<Point2>,
        style: LineStyle,
    },
    Surface3d {
        id: String,
        label: Option<String>,
        z: Expression,
        x_domain: Range,
        y_domain: Range,
        style: LineStyle,
    },
    Parametric3d {
        id: String,
        label: Option<String>,
        x: Expression,
        y: Expression,
        z: Expression,
        u: String,
        v: String,
        u_domain: Range,
        v_domain: Range,
        style: LineStyle,
    },
    Scatter3d {
        id: String,
        label: Option<String>,
        points: Vec<Point3>,
        style: LineStyle,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSpec {
    pub schema_version: String,
    pub dimension: GraphDimension,
    pub title: Option<String>,
    pub axes: GraphAxes,
    pub viewport: Viewport,
    pub interaction: GraphInteraction,
    pub series: Vec<GraphSeriesSpec>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
