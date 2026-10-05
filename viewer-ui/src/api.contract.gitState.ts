// The git-state unions the contract test checks at runtime — split out of
// `api.contract.test.ts` to keep it within the line limit. A JSON import
// widens every string, so these re-narrow the fixture's values against lists
// tied to the unions in both directions.

import type { Commit, RepoOperation } from "./api";

/** The same for git state: the operation a status carries, and a commit's
 *  ref kinds and divergence. */
const OPERATIONS = ["merge", "rebase", "cherry-pick", "revert", "bisect"] as const;
const REF_KINDS = ["head", "local", "tag", "remote"] as const;
const DIVERGENCES = ["ahead", "behind"] as const;

type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
export function gitNamesMatchTheUnions(): [
  Same<(typeof OPERATIONS)[number], RepoOperation["kind"]>,
  Same<(typeof REF_KINDS)[number], NonNullable<Commit["refs"]>[number]["kind"]>,
  Same<(typeof DIVERGENCES)[number], NonNullable<Commit["divergence"]>>,
] {
  return [true, true, true];
}

function oneOf<T extends string>(names: readonly T[], value: string): T {
  if (!(names as readonly string[]).includes(value)) {
    throw new Error(`fixture carries ${value}, which the client does not know`);
  }
  return value as T;
}

type RawOperation = { kind: string; step?: number; total?: number };
export function operation(raw: RawOperation | undefined): RepoOperation | undefined {
  return raw && { ...raw, kind: oneOf(OPERATIONS, raw.kind) };
}

type RawCommit = Omit<Commit, "refs" | "divergence"> & {
  refs?: { kind: string; name: string }[];
  divergence?: string;
};
export function commit(raw: RawCommit): Commit {
  return {
    ...raw,
    refs: raw.refs?.map((r) => ({ ...r, kind: oneOf(REF_KINDS, r.kind) })),
    divergence: raw.divergence === undefined ? undefined : oneOf(DIVERGENCES, raw.divergence),
  };
}

