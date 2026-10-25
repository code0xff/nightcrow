/** What to show in place of a status the server could not read. A directory
 *  that is not a repository yet is the common case and gets a sentence; any
 *  other failure is shown as the server worded it. */
export function unavailableMessage(reason: string): string {
  return reason.startsWith("not a git repository")
    ? "Not a git repository. Run git init here to track changes."
    : `Status unavailable: ${reason}`;
}
