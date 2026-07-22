import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
// highlight.js token colours for fenced code. Imported here (not in index.css)
// so it rides this lazily-loaded chunk instead of the initial bundle, and stays
// self-hosted under the viewer's `default-src 'self'` CSP. `.nc-markdown pre`
// owns the block's box; this file only paints the tokens inside it.
import "highlight.js/styles/github-dark.css";

/**
 * Rendered markdown for the file pane, lazily loaded (see `App.tsx`). Uses
 * `react-markdown`, which builds React elements from the parsed AST rather than
 * injecting an HTML string — so there is no `dangerouslySetInnerHTML` surface
 * and nothing to sanitise. `remark-gfm` adds GitHub's tables/tasklists/etc.;
 * `rehype-highlight` colours fenced code with highlight.js (also AST-based).
 *
 * Styling lives under `.nc-markdown` in `index.css`; only the wrapper class is
 * set here. Links are forced to open in a new tab with `noopener` so a rendered
 * document can never script the opener.
 */
export function MarkdownView({ source }: { source: string }) {
  return (
    <div className="nc-markdown p-4">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          a: ({ node: _node, ...props }) => (
            <a {...props} target="_blank" rel="noopener noreferrer" />
          ),
        }}
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
