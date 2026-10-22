## ADDED Requirements

### Requirement: Installing a package reloads
`bevy-ts dev` SHALL watch, besides every loaded module, each `package.json` the resolver read in the last compile, and, for each path the resolver looked for and did not find, the nearest folder that exists. A change to any of them SHALL start a normal reload; no other reload path SHALL exist for packages.

#### Scenario: Install after a missing package
- **WHEN** a load failed because `lodash-es` is not installed, and the user runs `pnpm add lodash-es` in the project
- **THEN** the engine reloads on its own and the game loads with `lodash-es`

#### Scenario: Editing a package's exports
- **WHEN** a linked package's `package.json` changes its `exports` while `bevy-ts dev` runs
- **THEN** the engine reloads and resolves the package's imports anew
