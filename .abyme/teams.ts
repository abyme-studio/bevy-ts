// bevy-ts's own roles and team templates (the teams plugin). A role's agent starts from its preset
// in the operator's config when work first reaches it: a repository's role (`lead@bevy-ts`) in the
// main checkout, kept open; a team's role in its team's worktree. The file is data: it is evaluated with
// nothing to reach. Lead brings a team up with `teams_up` and gives its first role the work. A copy of
// abyme's (~/dev/abyme/.abyme/teams.ts) on bevy-ts's presets.
import type { TeamFile } from "@abyme/teams";

const instructions =
  "Plain words, short sentences. One owner per concept. A change lands with its docs, spec and tests. Every role works in the team's one worktree: commit only your own paths. Ask the operator only as a question in your chat.";

export default {
  // lead@bevy-ts, the one the operator talks with, in the main checkout, bringing up a team per piece
  // of work.
  roles: { lead: "bevy-ts-lead" },
  templates: {
    // A change: one team per feature, named after it, gone when its branch lands.
    change: {
      title: "A change",
      instructions: `This team is one change, and you end with it: when its branch lands, or lead takes it down. ${instructions}`,
      ends: { "git.landed": { branch: "{branch}" } },
      groups: [
        { title: "Spec, design and research", roles: { spec: "bevy-ts-spec", design: "bevy-ts-design", research: "bevy-ts-research" } },
        { title: "Build", roles: { build: "bevy-ts-build" } },
        { title: "Review and walk", roles: { review: "bevy-ts-review", walk: "bevy-ts-walk" } },
      ],
    },
    // A question: research finds out, spec writes it into a proposal when it should become a change.
    // It ends when its first item, named after the team, is done or refused.
    research: {
      title: "Research",
      instructions: `This team is one question, and you end with it: when its work closes, or lead takes it down. ${instructions}`,
      ends: { "work.closed": { item: "{team}" } },
      groups: [{ title: "Research and spec", roles: { research: "bevy-ts-research", spec: "bevy-ts-spec" } }],
    },
  },
} satisfies TeamFile;
