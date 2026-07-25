export function PathLabel({
  path,
  from,
  className,
}: {
  path: string;
  from?: string;
  className?: string;
}) {
  return (
    <span
      className={`whitespace-nowrap ${className ?? ""}`}
      title={from ? `${from} → ${path}` : path}
    >
      {from ? `${from} → ${path}` : path}
    </span>
  );
}
