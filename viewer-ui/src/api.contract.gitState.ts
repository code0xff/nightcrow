// The git-state unions the contract test checks at runtime — split out of
// `api.contract.test.ts` to keep it within the line limit. A JSON import
// widens every string, so these re-narrow the fixture's values against lists
// tied to the unions in both directions.

import type { CommitRef, LogDecorations, RepoOperation } from "./api";

const OPERATIONS = ["merge", "rebase", "cherry-pick", "revert", "bisect", "am"] as const;
const REF_KINDS = ["head", "local", "tag", "remote"] as const;

type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
export function gitNamesMatchTheUnions(): [
  Same<(typeof OPERATIONS)[number], RepoOperation["kind"]>,
  Same<(typeof REF_KINDS)[number], CommitRef["kind"]>,
] {
  return [true, true];
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

type RawDecorations = Omit<LogDecorations, "refs"> & {
  refs: Record<string, { kind: string; name: string }[]>;
};
export function decorations(raw: RawDecorations): LogDecorations {
  return {
    ...raw,
    refs: Object.fromEntries(
      Object.entries(raw.refs).map(([oid, refs]) => [
        oid,
        refs.map((r) => ({ ...r, kind: oneOf(REF_KINDS, r.kind) })),
      ]),
    ),
  };
}
