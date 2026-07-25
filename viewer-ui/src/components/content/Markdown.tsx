import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
// Keep the token stylesheet in the lazy markdown chunk and self-hosted for CSP.
import "highlight.js/styles/github-dark.css";

/** AST-based rendering avoids HTML injection; links cannot script the opener. */
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
