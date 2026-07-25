export function Mark({ className }: { className?: string }) {
  return (
    <span
      className={`block overflow-hidden rounded-[20.7%] bg-accent ${className ?? ""}`}
    >
      <img src="/crow-mono.svg" alt="" aria-hidden="true" className="h-full w-full" />
    </span>
  );
}
