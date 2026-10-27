## ADDED Requirements

### Requirement: The atlas does not depend on the order labels are laid out in
The glyphs new in a frame SHALL be placed in the shared atlas in an order of their own (their rasters), not the order labels were laid out in that frame. The same labels spawned in one frame in any order SHALL make the same atlas, byte for byte, and give each label the same UVs. Two glyph rasters of the same pixels SHALL share one cell. Glyphs first used in different frames are placed in frame order.

#### Scenario: Labels spawned in another order
- **WHEN** six labels are spawned in one frame in one order, and in a fresh app in another
- **THEN** the two atlases are byte-identical and each label's UVs are equal
