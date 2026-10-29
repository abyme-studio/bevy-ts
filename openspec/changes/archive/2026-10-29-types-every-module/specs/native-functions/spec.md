## ADDED Requirements

### Requirement: bevy-ts types declares every built-in module
`bevy-ts types` SHALL declare, in `native-modules.d.ts`, every built-in module (`bevy-ts/<name>`) the engine registers when it runs (`bevy-ts dev`, `bevy-ts play`, a shipped game), and `bevy-ts build` SHALL accept an import of each. The run's app and `bevy-ts types` SHALL take the plugins that ship these modules from one list.

#### Scenario: The ready module
- **WHEN** `bevy-ts types` runs
- **THEN** `native-modules.d.ts` has `declare module "bevy-ts/ready"`, declaring `ReadyHold`

#### Scenario: No module missed
- **WHEN** the run's headless app is built in a process of its own, and `bevy-ts types` runs
- **THEN** every module the app registered is declared in `native-modules.d.ts`
