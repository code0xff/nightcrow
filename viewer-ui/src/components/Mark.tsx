// Inlined as a data URI (not a `/crow-mono.svg` fetch): the mark sits on the
// loading splash, which unmounts after a single `/api/repos` round trip, so a
// separately-fetched crow raced that unmount and flickered in and out depending
// on the network. Baked into the bundle it paints with the tile, every time —
// and stays correct even if `/crow-mono.svg` is not served.
import crowMonoDataUri from "../../public/crow-mono.svg?inline";

export function Mark({ className }: { className?: string }) {
  return (
    <span
      className={`block overflow-hidden rounded-[20.7%] bg-accent ${className ?? ""}`}
    >
      <img
        src={crowMonoDataUri}
        alt=""
        aria-hidden="true"
        className="h-full w-full"
      />
    </span>
  );
}
