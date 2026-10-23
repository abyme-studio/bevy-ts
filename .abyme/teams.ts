// bevy-ts's own roles and team templates (the teams plugin). A role's agent starts from its preset
// in the operator's config when work first reaches it: a repository's role (`lead@bevy-ts`) in the
// main checkout, kept open; a team's role in its team's worktree. The file is data: it is evaluated with
// nothing to reach. lead@bevy-ts brings a team up with `teams_up`; a change's own lead runs it from there. A copy of
// abyme's (~/dev/abyme/.abyme/teams.ts) on bevy-ts's presets.
import type { TeamFile } from "@abyme/teams";

const instructions =
  "Plain words, short sentences. One owner per concept. A change lands with its docs, spec and tests. Every role works in the team's one worktree: commit only your own paths. Ask the operator only as a question in your chat.";

export default {
  // lead@bevy-ts, the one the operator talks with, in the main checkout, bringing up a team per piece
  // of work.
  roles: { lead: "bevy-ts-lead" },
  templates: {
    // An area: a standing team with a lead and no worktree work of its own. It brings up change and
    // research teams as its children and runs them as lead@bevy-ts would (operator, 2026-10-23). It
    // has no end of its own: it lasts until lead@bevy-ts takes it down.
    area: {
      title: "An area",
      instructions: `This team is one area of bevy-ts, kept for as long as the area has work. ${instructions}`,
      lead: "bevy-ts-area-lead",
      groups: [{ title: "Research", roles: { research: "bevy-ts-research" } }],
    },
    // A change: one team per feature, named after it, its lead holding its root item. It ends when
    // the lead closes that item, the change archived: a change may land in stages.
    change: {
      title: "A change",
      instructions: `This team is one change, and you end with it: when your lead finishes it, or takes it down. ${instructions}`,
      lead: "bevy-ts-team-lead",
      ends: { "work.closed": { item: "{team}" } },
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
      instructions: `This team is one question, and you end with it: when its work closes, or lead@bevy-ts takes it down. ${instructions}`,
      ends: { "work.closed": { item: "{team}" } },
      groups: [{ title: "Research and spec", roles: { research: "bevy-ts-research", spec: "bevy-ts-spec" } }],
    },
  },
} satisfies TeamFile;
