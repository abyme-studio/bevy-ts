// The module an export claiming `ResMut<Wind>` ships, for claims.ts.
declare module "bevy-ts/claims-test" {
  export interface WindEdit {
    gust(strength: number): void;
  }
  export interface VirtualTime {
    pause(): void;
  }
}
