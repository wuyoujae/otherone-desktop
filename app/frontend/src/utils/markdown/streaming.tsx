// 流式 markdown 预解析渲染
// AI 边输出边检测 markdown 语法结构，提前渲染代码块等

import { useMemo } from 'react';
import { parseMarkdown } from './parser';
import { MarkdownRenderer } from './renderer';

/**
 * 在流式输出期间，识别已闭合的 markdown 块和正在输出的最后一个块。
 * 已闭合的块正常 markdown 渲染，正在输出的块用 blur-word 做入场动画。
 */
export function useStreamingMarkdown(content: string, isStreaming: boolean) {
  return useMemo(() => {
    if (!content) return null;
    if (!isStreaming) {
      const blocks = parseMarkdown(content);
      if (blocks.length === 0) return <p></p>;
      return <MarkdownRenderer blocks={blocks} />;
    }

    // --- 流式预解析 ---

    // 1. 检查是否有未闭合的代码围栏 (```)
    const fenceMatches = findAllFences(content);
    const hasOpenFence = fenceMatches.length % 2 === 1;

    if (hasOpenFence) {
      return renderOpenCodeBlock(content, fenceMatches);
    }

    // 2. 无代码块：以最后一个 \n\n 为界，之前的部分完整解析，之后的部分 blur-word
    const lastBoundary = content.lastIndexOf('\n\n');
    if (lastBoundary >= 0) {
      const stable = content.slice(0, lastBoundary).trim();
      const active = content.slice(lastBoundary + 2); // 跳过两个 \n
      const stableBlocks = stable.length > 0 ? parseMarkdown(stable) : [];

      return (
        <>
          {stableBlocks.length > 0 && <MarkdownRenderer blocks={stableBlocks} />}
          {active.length > 0 && <BlurLine text={active} />}
        </>
      );
    }

    // 3. 全部在流式输出中 → 纯 blur-word
    return <BlurLine text={content} />;
  }, [content, isStreaming]);
}

// ---- helpers ----

/** 找到所有以 ``` 开头的行的位置信息 */
function findAllFences(raw: string): Array<{ index: number; line: string }> {
  const results: Array<{ index: number; line: string }> = [];
  const re = /^```/gm;
  let match: RegExpExecArray | null;
  while ((match = re.exec(raw)) !== null) {
    results.push({ index: match.index, line: raw.slice(match.index).split('\n')[0] });
  }
  return results;
}

/** 渲染未闭合代码块：已写完的代码行 + 当前行 blur-word */
function renderOpenCodeBlock(
  raw: string,
  fences: Array<{ index: number; line: string }>,
) {
  const lastFence = fences[fences.length - 1];
  const fenceLine = lastFence.line;
  const lang = fenceLine.slice(3).trim();

  // 围栏之前的内容正常解析
  const beforeFence = raw.slice(0, lastFence.index).trim();
  const stableBlocks = beforeFence.length > 0 ? parseMarkdown(beforeFence) : [];

  // 围栏之后的内容（代码体 + 可能在最后一行正在敲）
  const codeBody = raw.slice(lastFence.index + fenceLine.length + 1); // 跳过 \n
  const codeLines = codeBody.split('\n');
  const lastLine = codeLines.length > 0 ? codeLines[codeLines.length - 1] : '';
  const completedLines = codeLines.slice(0, -1);
  const completedCode = completedLines.join('\n');

  return (
    <>
      {stableBlocks.length > 0 && <MarkdownRenderer blocks={stableBlocks} />}
      <div className="code-wrapper">
        <div className="code-header">
          <div className="code-header-left">
            <div className="mac-controls">
              <div className="mac-dot red" />
              <div className="mac-dot yellow" />
              <div className="mac-dot green" />
            </div>
            {lang && <span className="code-lang">{lang}</span>}
          </div>
          <span className="code-copy-btn" style={{ color: '#555', cursor: 'default' }}>
            ●● ●
          </span>
        </div>
        <pre>
          <code>
            {completedCode.length > 0 ? completedCode + '\n' : ''}
            <span className="blur-word">{lastLine}</span>
          </code>
        </pre>
      </div>
    </>
  );
}

/** 单行 blur-word 渲染 */
function BlurLine({ text }: { text: string }) {
  const tokens = text.split(/(\s+)/);
  return (
    <p>
      {tokens.map((token, index) => (
        <span key={index} className="blur-word">
          {token}
        </span>
      ))}
    </p>
  );
}
