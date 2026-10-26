## ADDED Requirements

### Requirement: Text along a path
`path` SHALL take a list of at least two points in the entity's local space. A label with a path SHALL place each glyph's baseline centre on the path at its advance along the path's arc length, its X axis along the path's tangent there, and its face toward `normal` (default `[0, 0, 1]`). `justify` SHALL place the text at the path's start, centre or end. Text longer than the path SHALL continue along the last segment's tangent. A path label SHALL be one line: `width` with `path`, or a line break in the text, SHALL fail the load naming the prop and location.

#### Scenario: Along an arc
- **WHEN** a label `ROME` has a `path` of 64 points on a half circle of radius 4 in the XY plane and `justify="center"`
- **THEN** each glyph's quad centre lies within `0.05 * size` of the circle's radius plus half the glyph's height, and each quad's X axis is within 2 degrees of the circle's tangent at its centre

#### Scenario: A path and a width
- **WHEN** a label sets both `path` and `width`
- **THEN** the load fails naming `<worldText>`, `width`, `path` and the element's file and line

## REMOVED Requirements

### Requirement: No text along a path yet
**Reason**: Labels take `path` and `normal` now (Text along a path).
**Migration**: None: the refusal only ever failed the load.
