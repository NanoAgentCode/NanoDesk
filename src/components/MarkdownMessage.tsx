import { useMemo } from "react";
import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import type { Plugin } from "unified";
import "katex/dist/katex.min.css";
import { openExternalUrl, openProjectFileLocation } from "../api";
import type { ProjectFileEntry } from "../types";

interface MarkdownMessageProps {
  content: string;
  projectPath?: string | null;
  projectFiles?: ProjectFileEntry[];
}

const LOCAL_FILE_EXTENSION = /\.(png|jpe?g|gif|webp|bmp|tiff?|svg|pdf|html?|css|jsx?|mjs|tsx?|json|md|txt|csv|ya?ml|toml|xml|lock|rs|py|java|go|cs|c|cpp|h|hpp|vue|svelte|sql|sh|ps1|bat|cmd|xlsx?|docx?|pptx?|zip)$/i;
const EXTERNAL_LINK_PROTOCOLS = new Set(["http:", "https:", "mailto:", "tel:"]);
const PATH_LINK_PATTERN =
  /(^|[\s([{"'，。；：、])((?:\.\/)?(?:[A-Za-z0-9_.-]+[\\/])*[A-Za-z0-9_.-]+\.(?:png|jpe?g|gif|webp|bmp|tiff?|svg|pdf|html?|css|jsx?|mjs|tsx?|json|md|txt|csv|ya?ml|toml|xml|lock|rs|py|java|go|cs|c|cpp|h|hpp|vue|svelte|sql|sh|ps1|bat|cmd|xlsx?|docx?|pptx?|zip)(?::\d+(?::\d+)?)?)(?=$|[\s)\]}"'，。；：、:,.!?！？])/gi;
const PATH_SEPARATOR = /[\\/]/;

type MarkdownNode = {
  type: string;
  value?: string;
  url?: string;
  title?: string | null;
  children?: MarkdownNode[];
};

function isProjectRelativeFileHref(href: string) {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#")) return false;
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) return false;
  const pathOnly = stripLineSuffix(trimmed.split(/[?#]/, 1)[0]).replace(/\\/g, "/");
  if (!pathOnly || pathOnly.startsWith("/") || pathOnly.includes(":")) return false;
  return LOCAL_FILE_EXTENSION.test(pathOnly);
}

function decodeHrefPath(href: string) {
  const trimmed = href.trim();
  if (/^file:/i.test(trimmed)) {
    try {
      const url = new URL(trimmed);
      const decodedPath = decodeURIComponent(url.pathname);
      return /^\/[A-Za-z]:\//.test(decodedPath) ? decodedPath.slice(1) : decodedPath;
    } catch {
      return "";
    }
  }

  return decodeProjectHref(trimmed);
}

function isAbsoluteLocalPath(path: string) {
  return /^[A-Za-z]:\//.test(path) || path.startsWith("/");
}

function resolveProjectFileHref(
  href: string,
  projectPath: string,
  fileNameIndex: Map<string, string>
) {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#") || EXTERNAL_LINK_PROTOCOLS.has(trimmed.split(":", 1)[0] + ":")) {
    return null;
  }

  const decodedPath = stripLineSuffix(decodeHrefPath(trimmed).split(/[?#]/, 1)[0])
    .replace(/\\/g, "/")
    .replace(/^\.\//, "");
  if (!decodedPath || !LOCAL_FILE_EXTENSION.test(decodedPath)) return null;

  if (!isAbsoluteLocalPath(decodedPath)) {
    if (/^[a-z][a-z0-9+.-]*:/i.test(decodedPath)) return null;
    return resolveAutoLinkedPath(decodedPath, fileNameIndex);
  }

  const normalizedProjectPath = projectPath.replace(/\\/g, "/").replace(/\/+$/, "");
  const caseInsensitive = /^[A-Za-z]:\//.test(normalizedProjectPath);
  const comparablePath = caseInsensitive ? decodedPath.toLowerCase() : decodedPath;
  const comparableProjectPath = caseInsensitive ? normalizedProjectPath.toLowerCase() : normalizedProjectPath;
  const projectPrefix = `${comparableProjectPath}/`;
  if (!comparablePath.startsWith(projectPrefix)) return null;

  return decodedPath.slice(normalizedProjectPath.length + 1);
}

function decodeProjectHref(href: string) {
  const pathOnly = href.trim().split(/[?#]/, 1)[0];
  try {
    return decodeURIComponent(pathOnly);
  } catch {
    return pathOnly;
  }
}

function stripLineSuffix(path: string) {
  return path.replace(/:\d+(?::\d+)?$/, "");
}

function normalizeAutoLinkedPath(path: string) {
  return stripLineSuffix(path).replace(/\\/g, "/").replace(/^\.\//, "");
}

function buildFileNameIndex(projectFiles: ProjectFileEntry[] = []) {
  const index = new Map<string, string>();

  projectFiles
    .filter((file) => !file.is_dir)
    .map((file) => file.path.replace(/\\/g, "/"))
    .sort((left, right) => {
      const leftDepth = left.split("/").length;
      const rightDepth = right.split("/").length;
      return leftDepth - rightDepth || left.length - right.length || left.localeCompare(right);
    })
    .forEach((path) => {
      const normalizedPath = path.toLowerCase();
      if (!index.has(normalizedPath)) {
        index.set(normalizedPath, path);
      }

      const fileName = path.split("/").pop()?.toLowerCase();
      if (fileName && !index.has(fileName)) {
        index.set(fileName, path);
      }
    });

  return index;
}

function resolveAutoLinkedPath(displayPath: string, fileNameIndex: Map<string, string>) {
  const normalizedPath = normalizeAutoLinkedPath(displayPath);
  if (PATH_SEPARATOR.test(stripLineSuffix(displayPath).replace(/^\.\//, ""))) {
    const exactPath = fileNameIndex.get(normalizedPath.toLowerCase());
    if (exactPath) return exactPath;

    const suffix = `/${normalizedPath.toLowerCase()}`;
    const suffixMatch = [...fileNameIndex.values()]
      .filter((path) => path.toLowerCase().endsWith(suffix))
      .sort((left, right) => left.length - right.length || left.localeCompare(right))[0];
    return suffixMatch || normalizedPath;
  }

  return fileNameIndex.get(normalizedPath.toLowerCase()) || normalizedPath;
}

function createTextNode(value: string): MarkdownNode {
  return { type: "text", value };
}

function createAutoLinkedPathNode(displayPath: string, fileNameIndex: Map<string, string>): MarkdownNode {
  return {
    type: "link",
    url: resolveAutoLinkedPath(displayPath, fileNameIndex),
    title: null,
    children: [createTextNode(displayPath)]
  };
}

function autoLinkProjectPathsInText(value: string, fileNameIndex: Map<string, string>) {
  const nodes: MarkdownNode[] = [];
  let cursor = 0;

  for (const match of value.matchAll(PATH_LINK_PATTERN)) {
    const matchedText = match[0];
    const prefix = match[1] || "";
    const displayPath = match[2];
    const pathStart = match.index + prefix.length;

    if (pathStart > cursor) {
      nodes.push(createTextNode(value.slice(cursor, pathStart)));
    }

    nodes.push(createAutoLinkedPathNode(displayPath, fileNameIndex));
    cursor = match.index + matchedText.length;
  }

  if (cursor === 0) return null;
  if (cursor < value.length) {
    nodes.push(createTextNode(value.slice(cursor)));
  }

  return nodes;
}

function autoLinkProjectPathsInNode(node: MarkdownNode, fileNameIndex: Map<string, string>) {
  if (!node.children || node.type === "link" || node.type === "linkReference") {
    return;
  }

  const nextChildren: MarkdownNode[] = [];
  let changed = false;

  for (const child of node.children) {
    if (child.type === "text" && child.value) {
      const linkedNodes = autoLinkProjectPathsInText(child.value, fileNameIndex);
      if (linkedNodes) {
        nextChildren.push(...linkedNodes);
        changed = true;
        continue;
      }
    }

    autoLinkProjectPathsInNode(child, fileNameIndex);
    nextChildren.push(child);
  }

  if (changed) {
    node.children = nextChildren;
  }
}

const remarkAutoLinkProjectPaths: Plugin<[Map<string, string>], MarkdownNode> = (fileNameIndex) => {
  return (tree) => {
    autoLinkProjectPathsInNode(tree, fileNameIndex);
  };
};

function getExternalResourceUrl(href: string) {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#")) return null;

  try {
    const url = new URL(trimmed);
    if (!EXTERNAL_LINK_PROTOCOLS.has(url.protocol)) return null;
    return url.toString();
  } catch {
    return null;
  }
}

function transformMarkdownUrl(url: string, key: string) {
  // react-markdown deliberately strips unknown protocols. Keep local file URLs and
  // Windows drive paths intact so the link renderer can safely resolve them against
  // the active project; all other protocols still use the library's safe defaults.
  if (key === "href" && (/^file:/i.test(url) || /^[A-Za-z]:(?:[\\/]|%5[Cc])/.test(url))) {
    return url;
  }
  return defaultUrlTransform(url);
}

function openLocalFileLocation(projectPath: string, relativePath: string) {
  void openProjectFileLocation(projectPath, relativePath).catch((error) => {
    console.error("Failed to open local file location:", error);
  });
}

function MarkdownMessage({ content, projectPath, projectFiles = [] }: MarkdownMessageProps) {
  const fileNameIndex = useMemo(() => buildFileNameIndex(projectFiles), [projectFiles]);

  // Pre-process content to convert single newlines to soft line breaks (two spaces followed by a newline)
  // in non-code blocks. This preserves natural line breaks during markdown rendering.
  const processedContent = useMemo(() => {
    if (!content) return "";
    const parts = content.split(/(```[\s\S]*?```|`[^`\n]*`|\$\$[\s\S]*?\$\$)/g);
    return parts
      .map((part) => {
        if (part.startsWith("```") || (part.startsWith("`") && part.endsWith("`"))) {
          return part;
        }
        if (part.startsWith("$$") && part.endsWith("$$")) {
          return `$$\n${part.slice(2, -2).trim()}\n$$`;
        }
        return part.replace(/(?<!\n)\n(?!\n)/g, "  \n");
      })
      .join("");
  }, [content]);

  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath, [remarkAutoLinkProjectPaths, fileNameIndex]]}
      rehypePlugins={[rehypeKatex]}
      urlTransform={transformMarkdownUrl}
      components={{
        a({ href, children, ...props }) {
          const relativePath = href && projectPath
            ? resolveProjectFileHref(href, projectPath, fileNameIndex)
            : null;
          if (relativePath && projectPath) {
            return (
              <button
                className="local-file-link"
                type="button"
                title={`打开所在文件夹: ${relativePath}`}
                onClick={() => openLocalFileLocation(projectPath, relativePath)}
              >
                {children}
              </button>
            );
          }

          const externalUrl = href ? getExternalResourceUrl(href) : null;
          if (externalUrl) {
            return (
              <a
                {...props}
                href={externalUrl}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(event) => {
                  event.preventDefault();
                  void openExternalUrl(externalUrl).catch((error) => {
                    console.error("Failed to open external resource:", error);
                  });
                }}
              >
                {children}
              </a>
            );
          }

          return (
            <a
              href={href}
              {...props}
              onClick={(event) => {
                // Never let an unrecognized Markdown link navigate the app WebView.
                // Relative/local navigation reloads the SPA and loses the current UI state.
                event.preventDefault();
              }}
            >
              {children}
            </a>
          );
        },
        code({ className, children, ...props }) {
          const match = /language-(\w+)/.exec(className || "");
          const language = match?.[1];
          const inlineText = String(children).trim();

          if (className) {
            return (
              <div className="code-block">
                <div className="code-block-header">
                  <span>{language || "code"}</span>
                </div>
                <pre>
                  <code className={className} {...props}>
                    {children}
                  </code>
                </pre>
              </div>
            );
          }

          if (projectPath && isProjectRelativeFileHref(inlineText)) {
            const relativePath = resolveAutoLinkedPath(decodeProjectHref(inlineText), fileNameIndex);
            return (
              <button
                className="inline-code local-file-code-link"
                type="button"
                title={`打开所在文件夹: ${relativePath}`}
                onClick={() => openLocalFileLocation(projectPath, relativePath)}
              >
                {children}
              </button>
            );
          }

          return (
            <code className="inline-code" {...props}>
              {children}
            </code>
          );
        }
      }}
    >
      {processedContent}
    </ReactMarkdown>
  );
}

export default MarkdownMessage;
