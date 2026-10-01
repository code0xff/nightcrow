import type { LinkState } from "./attachStatus";

export interface MutableFlag {
  current: boolean;
}

export function shouldRequestSizeForActivity(link: LinkState): boolean {
  return link === "live";
}

export function applySizeOwnerUpdate(
  ownsSize: MutableFlag,
  lastGeneration: { current: string | null },
  owned: boolean,
  generation: string | undefined,
  onAcquired: () => void,
): void {
  const generationChanged =
    generation !== undefined && generation !== lastGeneration.current;
  if (owned && (!ownsSize.current || generationChanged)) onAcquired();
  ownsSize.current = owned;
  lastGeneration.current = generation ?? null;
}
