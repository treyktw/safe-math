use crate::protocol::{
    AxisScale, GraphAxes, GraphDimension, GraphRequest, GraphSeries, GraphSeriesSpec, GraphSpec,
    LineStyle, Range, Viewport,
};
use async_trait::async_trait;
use engine_core::{
    Engine, EngineError, EngineHealth, EngineId, EngineKind, EngineMetadata, EngineResult,
};
use engine_math::{Expression, parse_equation, parse_expression};
use std::collections::{BTreeSet, HashSet};

const MAX_SERIES: usize = 32;
const MAX_POINTS: usize = 20_000;
const MAX_LABEL_CHARS: usize = 200;
const MAX_COORDINATE: f64 = 1.0e15;

#[async_trait]
pub trait GraphingEngine: Engine {
    async fn generate(&self, request: GraphRequest) -> EngineResult<GraphSpec>;
}

#[derive(Debug, Clone)]
pub struct SpecificationGraphingEngine {
    metadata: EngineMetadata,
}

impl SpecificationGraphingEngine {
    pub fn new() -> EngineResult<Self> {
        let metadata = EngineMetadata::new(
            EngineId::new("graphing.specification")?,
            EngineKind::Graphing,
            "Graph specification engine",
        )
        .with_capabilities([
            "function_2d",
            "parametric_2d",
            "polar_2d",
            "implicit_2d",
            "scatter_2d",
            "surface_3d",
            "parametric_3d",
            "scatter_3d",
            "interactive_specification",
        ]);
        Ok(Self { metadata })
    }
}

#[async_trait]
impl Engine for SpecificationGraphingEngine {
    fn metadata(&self) -> EngineMetadata {
        self.metadata.clone()
    }

    async fn health(&self) -> EngineHealth {
        EngineHealth::healthy(0)
    }
}

#[async_trait]
impl GraphingEngine for SpecificationGraphingEngine {
    async fn generate(&self, request: GraphRequest) -> EngineResult<GraphSpec> {
        validate_label(request.title.as_deref(), "title")?;
        if request.series.is_empty() {
            return Err(EngineError::invalid_input(
                "graph_series_missing",
                "a graph requires at least one series",
            ));
        }
        if request.series.len() > MAX_SERIES {
            return Err(EngineError::capacity(
                "graph_too_many_series",
                format!("a graph may contain at most {MAX_SERIES} series"),
            ));
        }

        let mut ids = HashSet::new();
        let mut dimension = None;
        let mut total_points = 0usize;
        let mut series = Vec::with_capacity(request.series.len());
        for item in request.series {
            let item_dimension = series_dimension(&item);
            if dimension.is_some_and(|current| current != item_dimension) {
                return Err(EngineError::invalid_input(
                    "graph_mixed_dimensions",
                    "2D and 3D series must be placed in separate graph specifications",
                ));
            }
            dimension = Some(item_dimension);
            let id = series_id(&item);
            validate_id(id)?;
            if !ids.insert(id.to_owned()) {
                return Err(EngineError::conflict(
                    "graph_series_id_duplicate",
                    format!("series id '{id}' appears more than once"),
                ));
            }
            validate_label(series_label(&item), "series label")?;
            total_points = total_points.saturating_add(point_count(&item));
            if total_points > MAX_POINTS {
                return Err(EngineError::capacity(
                    "graph_too_many_points",
                    format!("scatter series may contain at most {MAX_POINTS} points in total"),
                ));
            }
            series.push(prepare_series(item)?);
        }

        let dimension = dimension.expect("non-empty series checked above");
        let axes = request.axes.unwrap_or_else(|| default_axes(dimension));
        validate_axes(&axes, dimension)?;
        let viewport = request
            .viewport
            .unwrap_or_else(|| default_viewport(dimension));
        validate_viewport(&viewport, dimension, &axes)?;

        Ok(GraphSpec {
            schema_version: "graph.v1".into(),
            dimension,
            title: request.title,
            axes,
            viewport,
            interaction: request.interaction,
            series,
            warnings: Vec::new(),
        })
    }
}

fn series_dimension(series: &GraphSeries) -> GraphDimension {
    match series {
        GraphSeries::Function2d { .. }
        | GraphSeries::Parametric2d { .. }
        | GraphSeries::Polar2d { .. }
        | GraphSeries::Implicit2d { .. }
        | GraphSeries::Scatter2d { .. } => GraphDimension::TwoD,
        GraphSeries::Surface3d { .. }
        | GraphSeries::Parametric3d { .. }
        | GraphSeries::Scatter3d { .. } => GraphDimension::ThreeD,
    }
}

fn series_id(series: &GraphSeries) -> &str {
    match series {
        GraphSeries::Function2d { id, .. }
        | GraphSeries::Parametric2d { id, .. }
        | GraphSeries::Polar2d { id, .. }
        | GraphSeries::Implicit2d { id, .. }
        | GraphSeries::Scatter2d { id, .. }
        | GraphSeries::Surface3d { id, .. }
        | GraphSeries::Parametric3d { id, .. }
        | GraphSeries::Scatter3d { id, .. } => id,
    }
}

fn series_label(series: &GraphSeries) -> Option<&str> {
    match series {
        GraphSeries::Function2d { label, .. }
        | GraphSeries::Parametric2d { label, .. }
        | GraphSeries::Polar2d { label, .. }
        | GraphSeries::Implicit2d { label, .. }
        | GraphSeries::Scatter2d { label, .. }
        | GraphSeries::Surface3d { label, .. }
        | GraphSeries::Parametric3d { label, .. }
        | GraphSeries::Scatter3d { label, .. } => label.as_deref(),
    }
}

fn point_count(series: &GraphSeries) -> usize {
    match series {
        GraphSeries::Scatter2d { points, .. } => points.len(),
        GraphSeries::Scatter3d { points, .. } => points.len(),
        _ => 0,
    }
}

fn prepare_series(series: GraphSeries) -> EngineResult<GraphSeriesSpec> {
    Ok(match series {
        GraphSeries::Function2d {
            id,
            label,
            expression,
            variable,
            domain,
            style,
        } => {
            validate_range(domain, "domain")?;
            validate_style(&style)?;
            let expression = expression_with_symbols(&expression, &[variable.as_str()])?;
            GraphSeriesSpec::Function2d {
                id,
                label,
                expression,
                variable,
                domain,
                style,
            }
        }
        GraphSeries::Parametric2d {
            id,
            label,
            x,
            y,
            parameter,
            domain,
            style,
        } => {
            validate_range(domain, "domain")?;
            validate_style(&style)?;
            let allowed = [parameter.as_str()];
            GraphSeriesSpec::Parametric2d {
                id,
                label,
                x: expression_with_symbols(&x, &allowed)?,
                y: expression_with_symbols(&y, &allowed)?,
                parameter,
                domain,
                style,
            }
        }
        GraphSeries::Polar2d {
            id,
            label,
            radius,
            angle,
            domain,
            style,
        } => {
            validate_range(domain, "domain")?;
            validate_style(&style)?;
            let radius = expression_with_symbols(&radius, &[angle.as_str()])?;
            GraphSeriesSpec::Polar2d {
                id,
                label,
                radius,
                angle,
                domain,
                style,
            }
        }
        GraphSeries::Implicit2d {
            id,
            label,
            equation,
            x_domain,
            y_domain,
            style,
        } => {
            validate_range(x_domain, "xDomain")?;
            validate_range(y_domain, "yDomain")?;
            validate_style(&style)?;
            let equation = parse_equation(&equation)?;
            validate_symbols(equation.symbols(), &["x", "y"])?;
            GraphSeriesSpec::Implicit2d {
                id,
                label,
                equation,
                x_domain,
                y_domain,
                style,
            }
        }
        GraphSeries::Scatter2d {
            id,
            label,
            points,
            style,
        } => {
            validate_style(&style)?;
            if points.is_empty() {
                return Err(EngineError::invalid_input(
                    "graph_points_missing",
                    "a scatter series requires at least one point",
                ));
            }
            for point in &points {
                validate_coordinate(point.x)?;
                validate_coordinate(point.y)?;
            }
            GraphSeriesSpec::Scatter2d {
                id,
                label,
                points,
                style,
            }
        }
        GraphSeries::Surface3d {
            id,
            label,
            z,
            x_domain,
            y_domain,
            style,
        } => {
            validate_range(x_domain, "xDomain")?;
            validate_range(y_domain, "yDomain")?;
            validate_style(&style)?;
            GraphSeriesSpec::Surface3d {
                id,
                label,
                z: expression_with_symbols(&z, &["x", "y"])?,
                x_domain,
                y_domain,
                style,
            }
        }
        GraphSeries::Parametric3d {
            id,
            label,
            x,
            y,
            z,
            u,
            v,
            u_domain,
            v_domain,
            style,
        } => {
            validate_range(u_domain, "uDomain")?;
            validate_range(v_domain, "vDomain")?;
            validate_style(&style)?;
            let allowed = [u.as_str(), v.as_str()];
            GraphSeriesSpec::Parametric3d {
                id,
                label,
                x: expression_with_symbols(&x, &allowed)?,
                y: expression_with_symbols(&y, &allowed)?,
                z: expression_with_symbols(&z, &allowed)?,
                u,
                v,
                u_domain,
                v_domain,
                style,
            }
        }
        GraphSeries::Scatter3d {
            id,
            label,
            points,
            style,
        } => {
            validate_style(&style)?;
            if points.is_empty() {
                return Err(EngineError::invalid_input(
                    "graph_points_missing",
                    "a scatter series requires at least one point",
                ));
            }
            for point in &points {
                validate_coordinate(point.x)?;
                validate_coordinate(point.y)?;
                validate_coordinate(point.z)?;
            }
            GraphSeriesSpec::Scatter3d {
                id,
                label,
                points,
                style,
            }
        }
    })
}

fn expression_with_symbols(source: &str, allowed: &[&str]) -> EngineResult<Expression> {
    for variable in allowed {
        validate_variable(variable)?;
    }
    let expression = parse_expression(source)?;
    validate_symbols(expression.symbols(), allowed)?;
    Ok(expression)
}

fn validate_variable(value: &str) -> EngineResult<()> {
    match parse_expression(value)? {
        Expression::Symbol { name } if name == value => Ok(()),
        _ => Err(EngineError::invalid_input(
            "graph_variable_invalid",
            format!("'{value}' is not a valid graph variable name"),
        )),
    }
}

fn validate_symbols(symbols: BTreeSet<String>, allowed: &[&str]) -> EngineResult<()> {
    let unknown = symbols
        .into_iter()
        .filter(|symbol| !allowed.contains(&symbol.as_str()))
        .collect::<Vec<_>>();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(EngineError::invalid_input(
        "graph_unknown_symbol",
        format!(
            "expression contains symbols not defined by this series: {}",
            unknown.join(", ")
        ),
    ))
}

fn validate_id(id: &str) -> EngineResult<()> {
    if id.is_empty() || id.len() > 80 {
        return Err(EngineError::invalid_input(
            "graph_series_id_invalid",
            "series ids must contain between 1 and 80 bytes",
        ));
    }
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(EngineError::invalid_input(
            "graph_series_id_invalid",
            "series ids may only contain ASCII letters, numbers, dashes, underscores, and dots",
        ));
    }
    Ok(())
}

fn validate_label(value: Option<&str>, field: &str) -> EngineResult<()> {
    if value.is_some_and(|value| value.chars().count() > MAX_LABEL_CHARS) {
        return Err(EngineError::capacity(
            "graph_label_too_long",
            format!("{field} may contain at most {MAX_LABEL_CHARS} characters"),
        ));
    }
    Ok(())
}

fn validate_style(style: &LineStyle) -> EngineResult<()> {
    if !style.width.is_finite() || !(0.1..=20.0).contains(&style.width) {
        return Err(EngineError::invalid_input(
            "graph_style_width_invalid",
            "line width must be between 0.1 and 20",
        ));
    }
    if !style.opacity.is_finite() || !(0.0..=1.0).contains(&style.opacity) {
        return Err(EngineError::invalid_input(
            "graph_style_opacity_invalid",
            "opacity must be between 0 and 1",
        ));
    }
    if let Some(color) = &style.color {
        let hex = color.strip_prefix('#').is_some_and(|digits| {
            matches!(digits.len(), 3 | 4 | 6 | 8)
                && digits
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        });
        let token = !color.is_empty()
            && color.len() <= 32
            && color.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            });
        if !hex && !token {
            return Err(EngineError::invalid_input(
                "graph_style_color_invalid",
                "colors must be a hex value or a renderer theme token",
            ));
        }
    }
    Ok(())
}

fn validate_coordinate(value: f64) -> EngineResult<()> {
    if !value.is_finite() || value.abs() > MAX_COORDINATE {
        return Err(EngineError::invalid_input(
            "graph_coordinate_invalid",
            "coordinates must be finite and have magnitude at most 1e15",
        ));
    }
    Ok(())
}

fn validate_range(range: Range, name: &str) -> EngineResult<()> {
    validate_coordinate(range.min)?;
    validate_coordinate(range.max)?;
    if range.min >= range.max {
        return Err(EngineError::invalid_input(
            "graph_range_invalid",
            format!("{name}.min must be less than {name}.max"),
        ));
    }
    Ok(())
}

fn validate_axes(axes: &GraphAxes, dimension: GraphDimension) -> EngineResult<()> {
    for (name, axis) in [("x", &axes.x), ("y", &axes.y)] {
        validate_label(axis.label.as_deref(), &format!("{name} axis label"))?;
        validate_label(axis.unit.as_deref(), &format!("{name} axis unit"))?;
    }
    match (dimension, axes.z.as_ref()) {
        (GraphDimension::ThreeD, None) => {
            return Err(EngineError::invalid_input(
                "graph_z_axis_missing",
                "a 3D graph requires a z axis",
            ));
        }
        (GraphDimension::TwoD, Some(_)) => {
            return Err(EngineError::invalid_input(
                "graph_z_axis_unexpected",
                "a 2D graph must not define a z axis",
            ));
        }
        (_, Some(axis)) => {
            validate_label(axis.label.as_deref(), "z axis label")?;
            validate_label(axis.unit.as_deref(), "z axis unit")?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_viewport(
    viewport: &Viewport,
    dimension: GraphDimension,
    axes: &GraphAxes,
) -> EngineResult<()> {
    validate_range(viewport.x, "viewport.x")?;
    validate_range(viewport.y, "viewport.y")?;
    validate_log_range(axes.x.scale, viewport.x, "x")?;
    validate_log_range(axes.y.scale, viewport.y, "y")?;
    match (dimension, viewport.z, axes.z.as_ref()) {
        (GraphDimension::ThreeD, Some(z), Some(axis)) => {
            validate_range(z, "viewport.z")?;
            validate_log_range(axis.scale, z, "z")?;
        }
        (GraphDimension::ThreeD, None, _) => {
            return Err(EngineError::invalid_input(
                "graph_z_viewport_missing",
                "a 3D graph requires a z viewport",
            ));
        }
        (GraphDimension::TwoD, Some(_), _) => {
            return Err(EngineError::invalid_input(
                "graph_z_viewport_unexpected",
                "a 2D graph must not define a z viewport",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_log_range(scale: AxisScale, range: Range, axis: &str) -> EngineResult<()> {
    if scale == AxisScale::Logarithmic && range.min <= 0.0 {
        return Err(EngineError::invalid_input(
            "graph_log_range_invalid",
            format!("the {axis} axis uses a logarithmic scale and requires a positive viewport"),
        ));
    }
    Ok(())
}

fn default_axes(dimension: GraphDimension) -> GraphAxes {
    GraphAxes {
        x: Default::default(),
        y: Default::default(),
        z: (dimension == GraphDimension::ThreeD).then(Default::default),
    }
}

fn default_viewport(dimension: GraphDimension) -> Viewport {
    Viewport {
        x: Range::new(-10.0, 10.0),
        y: Range::new(-10.0, 10.0),
        z: (dimension == GraphDimension::ThreeD).then(|| Range::new(-10.0, 10.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{GraphInteraction, Point3};

    fn request(series: Vec<GraphSeries>) -> GraphRequest {
        GraphRequest {
            title: Some("Test".into()),
            series,
            axes: None,
            viewport: None,
            interaction: GraphInteraction::default(),
        }
    }

    #[tokio::test]
    async fn creates_a_safe_2d_function_specification() {
        let engine = SpecificationGraphingEngine::new().unwrap();
        let spec = engine
            .generate(request(vec![GraphSeries::Function2d {
                id: "parabola".into(),
                label: Some("y = x²".into()),
                expression: "x^2".into(),
                variable: "x".into(),
                domain: Range::new(-10.0, 10.0),
                style: LineStyle::default(),
            }]))
            .await
            .unwrap();
        assert_eq!(spec.schema_version, "graph.v1");
        assert_eq!(spec.dimension, GraphDimension::TwoD);
        assert!(matches!(spec.series[0], GraphSeriesSpec::Function2d { .. }));
    }

    #[test]
    fn documented_json_shape_deserializes() {
        let request: GraphRequest = serde_json::from_value(serde_json::json!({
            "title": "Parabola",
            "series": [{
                "kind": "function2d",
                "id": "parabola",
                "label": "y = x²",
                "expression": "x^2",
                "variable": "x",
                "domain": { "min": -10, "max": 10 }
            }]
        }))
        .unwrap();
        assert_eq!(request.series.len(), 1);
    }

    #[tokio::test]
    async fn creates_a_3d_scatter_specification() {
        let engine = SpecificationGraphingEngine::new().unwrap();
        let spec = engine
            .generate(request(vec![GraphSeries::Scatter3d {
                id: "samples".into(),
                label: None,
                points: vec![Point3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                }],
                style: LineStyle::default(),
            }]))
            .await
            .unwrap();
        assert_eq!(spec.dimension, GraphDimension::ThreeD);
        assert!(spec.axes.z.is_some());
        assert!(spec.viewport.z.is_some());
    }

    #[tokio::test]
    async fn rejects_undeclared_expression_symbols() {
        let engine = SpecificationGraphingEngine::new().unwrap();
        let error = engine
            .generate(request(vec![GraphSeries::Function2d {
                id: "bad".into(),
                label: None,
                expression: "x + secret".into(),
                variable: "x".into(),
                domain: Range::new(-1.0, 1.0),
                style: LineStyle::default(),
            }]))
            .await
            .unwrap_err();
        assert_eq!(error.code, "graph_unknown_symbol");
    }

    #[tokio::test]
    async fn rejects_renderer_unsafe_color_values() {
        let engine = SpecificationGraphingEngine::new().unwrap();
        let error = engine
            .generate(request(vec![GraphSeries::Function2d {
                id: "bad-color".into(),
                label: None,
                expression: "x".into(),
                variable: "x".into(),
                domain: Range::new(-1.0, 1.0),
                style: LineStyle {
                    color: Some("url(javascript:alert(1))".into()),
                    ..LineStyle::default()
                },
            }]))
            .await
            .unwrap_err();
        assert_eq!(error.code, "graph_style_color_invalid");
    }

    #[tokio::test]
    async fn rejects_mixed_2d_and_3d_series() {
        let engine = SpecificationGraphingEngine::new().unwrap();
        let error = engine
            .generate(request(vec![
                GraphSeries::Scatter2d {
                    id: "flat".into(),
                    label: None,
                    points: vec![crate::protocol::Point2 { x: 0.0, y: 0.0 }],
                    style: LineStyle::default(),
                },
                GraphSeries::Scatter3d {
                    id: "space".into(),
                    label: None,
                    points: vec![Point3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }],
                    style: LineStyle::default(),
                },
            ]))
            .await
            .unwrap_err();
        assert_eq!(error.code, "graph_mixed_dimensions");
    }
}
