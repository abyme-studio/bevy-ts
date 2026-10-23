## ADDED Requirements

### Requirement: Kinds bring their own prepass shaders
`material()` SHALL take `prepassFragment` and `prepassVertex`, shader files from `.wgsl` imports as `fragment` and `vertex` are, used by the kind in the prepass and the shadow pass, as Bevy's `Material::prepass_fragment_shader` and `prepass_vertex_shader` are. Their source SHALL receive the same appended params and texture declarations. A `{ mask }` kind with a `prepassFragment` SHALL default `shadows` and `prepass` to true; a kind with a `vertex` SHALL default them to true when it has a `prepassVertex` (and, if masked, a `prepassFragment`). An explicit `shadows` or `prepass` SHALL override. Each new stage SHALL hot reload in place and report WGSL errors at the author's `file:line` as `fragment` and `vertex` do, checked under the shader defs of the prepass or shadow pipeline that draws it.

#### Scenario: A leaf card casts a leaf
- **WHEN** a `{ mask: 0.5 }` kind on a quad samples a leaf texture in `fragment`, and its `prepassFragment` samples the same texture and discards below the cutoff
- **THEN** the quad's shadow on the ground has the leaf's outline, not the quad's, and ink from the prepass follows the leaf

#### Scenario: Wind sway in the shadow
- **WHEN** a kind's `vertex` and `prepassVertex` move vertices by the same sway
- **THEN** its shadow moves with the drawn mesh

#### Scenario: A prepass shader error
- **WHEN** `leaf-prepass.wgsl` has a type error on line 9 and is saved
- **THEN** the error names the kind and `src/leaf-prepass.wgsl:9`, and the previous prepass shader keeps drawing
