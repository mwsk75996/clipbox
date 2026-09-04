// ----------
// Code Detection & Highlighting
// Description: Conservative code gating plus highlight.js rendering for expanded text cards. Only registered languages participate in auto-detect.
// ----------

import hljs from "highlight.js/lib/core";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import csharp from "highlight.js/lib/languages/csharp";
import css from "highlight.js/lib/languages/css";
import go from "highlight.js/lib/languages/go";
import ini from "highlight.js/lib/languages/ini";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import powershell from "highlight.js/lib/languages/powershell";
import bash from "highlight.js/lib/languages/bash";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

hljs.registerLanguage("c", c);
hljs.registerLanguage("cpp", cpp);
hljs.registerLanguage("csharp", csharp);
hljs.registerLanguage("css", css);
hljs.registerLanguage("go", go);
hljs.registerLanguage("ini", ini);
hljs.registerLanguage("java", java);
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("json", json);
hljs.registerLanguage("powershell", powershell);
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("python", python);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("sql", sql);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("yaml", yaml);

const CODE_SIGNALS =
  /[{;}()=>]|->|::|#include|^\s*(function|const|let|var|def|class|import|from|export|return|if|for|while|switch|try|catch|fn|struct|impl|using|namespace|public|private|static|void|int|string|bool|SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER)\b/im;

/// Conservative gate so prose, chats, and URLs never get "highlighted".
/// Requires substance (multi-line or long) plus at least one code signal.
export function looksLikeCode(content: string): boolean {
  const text = content.trim();
  if (text.length < 20) return false;
  if (/^https?:\/\//i.test(text)) return false;
  const lines = text.split(/\r?\n/);
  if (lines.length < 2 && text.length < 120) return false;
  return CODE_SIGNALS.test(text);
}

/// Highlight.js escapes input HTML, so the output is safe for innerHTML.
export function highlightCode(content: string): string {
  return hljs.highlightAuto(content).value;
}
