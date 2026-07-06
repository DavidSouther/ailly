"use strict";

/* ---------- tiny dependency-free markdown-ish renderer ---------- */
function escapeHtml(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderInline(s) {
  s = s.replace(/`([^`]+)`/g, (_, code) => `<code>${code}</code>`);
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  s = s.replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, "$1<em>$2</em>");
  return s;
}

function renderMarkdownish(raw) {
  if (!raw) return "";
  const escaped = escapeHtml(raw);
  const lines = escaped.split("\n");
  let html = "";
  let inCode = false;
  let listType = null; // 'ul' | 'ol' | null
  let paragraph = [];

  function flushParagraph() {
    if (paragraph.length) {
      html += `<p>${paragraph.join("<br>")}</p>`;
      paragraph = [];
    }
  }
  function closeList() {
    if (listType) {
      html += `</${listType}>`;
      listType = null;
    }
  }

  for (const line of lines) {
    if (line.trim().startsWith("```")) {
      if (!inCode) {
        flushParagraph();
        closeList();
        html += "<pre><code>";
        inCode = true;
      } else {
        html += "</code></pre>";
        inCode = false;
      }
      continue;
    }
    if (inCode) {
      html += line + "\n";
      continue;
    }
    const headerMatch = line.match(/^(#{1,4})\s+(.*)$/);
    if (headerMatch) {
      flushParagraph();
      closeList();
      const level = headerMatch[1].length;
      html += `<h${level}>${renderInline(headerMatch[2])}</h${level}>`;
      continue;
    }
    const bulletMatch = line.match(/^\s*[-*]\s+(.*)$/);
    const numberedMatch = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (bulletMatch) {
      flushParagraph();
      if (listType !== "ul") {
        closeList();
        html += "<ul>";
        listType = "ul";
      }
      html += `<li>${renderInline(bulletMatch[1])}</li>`;
      continue;
    }
    if (numberedMatch) {
      flushParagraph();
      if (listType !== "ol") {
        closeList();
        html += "<ol>";
        listType = "ol";
      }
      html += `<li>${renderInline(numberedMatch[1])}</li>`;
      continue;
    }
    if (line.trim() === "") {
      flushParagraph();
      closeList();
      continue;
    }
    paragraph.push(renderInline(line));
  }
  flushParagraph();
  closeList();
  if (inCode) html += "</code></pre>";
  return html;
}

/* ---------------------------- API ---------------------------- */
const api = {
  async state() {
    return (await fetch("/api/state")).json();
  },
  async list(filters) {
    const params = new URLSearchParams();
    if (filters.source) params.set("source", filters.source);
    if (filters.status) params.set("status", filters.status);
    if (filters.q) params.set("q", filters.q);
    return (await fetch(`/api/candidates?${params}`)).json();
  },
  async candidate(id) {
    const res = await fetch(`/api/candidate/${encodeURIComponent(id)}`);
    if (!res.ok) return null;
    return res.json();
  },
  async conversation(id) {
    const res = await fetch(`/api/candidate/${encodeURIComponent(id)}/conversation`);
    if (!res.ok) return "";
    return res.text();
  },
  async grade(id, label) {
    const res = await fetch("/api/grade", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id, label }),
    });
    return res.json();
  },
  async setIncluded(id, included) {
    const res = await fetch("/api/include", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id, included }),
    });
    return res.json();
  },
  async exportSet() {
    const res = await fetch("/api/export", { method: "POST" });
    return res.json();
  },
};

/* --------------------------- state --------------------------- */
const els = {
  progress: document.getElementById("progress"),
  list: document.getElementById("candidate-list"),
  filterSource: document.getElementById("filter-source"),
  filterStatus: document.getElementById("filter-status"),
  jumpForm: document.getElementById("jump-form"),
  jumpInput: document.getElementById("jump-input"),
  emptyState: document.getElementById("empty-state"),
  card: document.getElementById("candidate-card"),
  badges: document.getElementById("badges"),
  metaId: document.getElementById("meta-id"),
  metaSource: document.getElementById("meta-source"),
  metaModel: document.getElementById("meta-model"),
  metaProject: document.getElementById("meta-project"),
  metaTurn: document.getElementById("meta-turn"),
  metaInvocation: document.getElementById("meta-invocation"),
  questionBody: document.getElementById("question-body"),
  responseBody: document.getElementById("response-body"),
  conversationBody: document.getElementById("conversation-body"),
  btnPass: document.getElementById("btn-pass"),
  btnFail: document.getElementById("btn-fail"),
  btnSkip: document.getElementById("btn-skip"),
  includeCheckbox: document.getElementById("include-checkbox"),
  exportBtn: document.getElementById("export-btn"),
  toast: document.getElementById("toast"),
};

let allCandidates = []; // last fetched summaries (unfiltered by status, filtered by source/q)
let currentId = null;
let currentDetail = null;
const skippedThisRound = new Set();
let toastTimer = null;

function showToast(msg, ms = 3200) {
  els.toast.textContent = msg;
  els.toast.hidden = false;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { els.toast.hidden = true; }, ms);
}

function currentFilters() {
  return {
    source: els.filterSource.value || null,
    status: els.filterStatus.value || null,
    q: null,
  };
}

async function refreshState() {
  const s = await api.state();
  els.progress.innerHTML =
    `<strong>${s.graded}</strong> / ${s.total} graded &nbsp;·&nbsp; ` +
    `<strong>${s.remaining}</strong> remaining &nbsp;·&nbsp; ` +
    `${s.passed} pass / ${s.failed} fail &nbsp;·&nbsp; ` +
    `${s.included_count} marked for export` +
    (s.suggestions_count
      ? ` &nbsp;·&nbsp; ${s.suggestions_count} heuristic suggestions (as of ${s.suggestions_computed_at_grade_count} grades)`
      : ` &nbsp;·&nbsp; heuristic suggestions unlock at ${s.next_suggestion_recompute_at} grades`);
  return s;
}

async function refreshList() {
  const filters = currentFilters();
  const { candidates } = await api.list(filters);
  allCandidates = candidates;
  renderList();
  return candidates;
}

function renderList() {
  els.list.innerHTML = "";
  for (const c of allCandidates) {
    const li = document.createElement("li");
    li.dataset.id = c.id;
    if (c.id === currentId) li.classList.add("active");
    const chipClass = c.label === "pass" ? "pass" : c.label === "fail" ? "fail" : "todo";
    li.innerHTML = `
      <div class="row1">
        <span class="chip ${chipClass}">${c.label}</span>
        <span>${c.source}${c.has_implied ? ' <span title="human-implied hint">💡</span>' : ""}${c.suggestion ? '<span class="dot-suggestion" title="heuristic suggestion available"></span>' : ""}${c.included ? " ⭐" : ""}</span>
      </div>
      <div class="cid">${c.id}</div>
    `;
    li.addEventListener("click", () => loadCandidate(c.id));
    els.list.appendChild(li);
  }
}

function nextUngradedId() {
  const pool = allCandidates.filter((c) => c.label === "TODO" && !skippedThisRound.has(c.id));
  if (pool.length === 0) {
    if (skippedThisRound.size > 0) {
      // Everything left has been skipped this round; give skipped ones another lap.
      skippedThisRound.clear();
      return nextUngradedId();
    }
    return null;
  }
  return pool[0].id;
}

function renderBadges(detail) {
  els.badges.innerHTML = "";
  const implied = detail.candidate_label_human_implied;
  if (implied) {
    const div = document.createElement("div");
    div.className = "badge implied";
    div.innerHTML = `
      <span class="badge-title">Suggestion — weak signal from human's next message</span>
      implied verdict: <strong>${implied.verdict}</strong>
      (evidence: “${escapeHtml(implied.evidence_phrase || "")}”, confidence: ${implied.confidence || "low"}).
      This is <em>not</em> a confirmed label — please review and grade explicitly.
    `;
    els.badges.appendChild(div);
  }
  const suggestion = detail.suggestion;
  if (suggestion) {
    const div = document.createElement("div");
    div.className = "badge heuristic";
    const neighborList = (suggestion.neighbors || [])
      .map((n) => `${n.id} (${n.label}, sim ${n.similarity})`)
      .join("; ");
    div.innerHTML = `
      <span class="badge-title">Suggestion — similarity heuristic</span>
      suggested label: <strong>${suggestion.label}</strong>
      (${Math.round(suggestion.confidence * 100)}% of top-${suggestion.k} neighbors agree,
      closest match <code>${suggestion.neighbor_id}</code> [${suggestion.neighbor_label}] at similarity ${suggestion.top_similarity}).
      <div class="neighbors">neighbors: ${neighborList}</div>
    `;
    els.badges.appendChild(div);
  }
}

function highlightSuggestedButton(detail) {
  els.btnPass.classList.remove("suggested-pass");
  els.btnFail.classList.remove("suggested-fail");
  const suggestedLabel =
    (detail.suggestion && detail.suggestion.label) ||
    (detail.candidate_label_human_implied && detail.candidate_label_human_implied.verdict);
  if (suggestedLabel === "pass") els.btnPass.classList.add("suggested-pass");
  if (suggestedLabel === "fail") els.btnFail.classList.add("suggested-fail");
}

async function loadCandidate(id) {
  const detail = await api.candidate(id);
  if (!detail) {
    showToast(`Unknown candidate id: ${id}`);
    return;
  }
  currentId = id;
  currentDetail = detail;
  els.emptyState.hidden = true;
  els.card.hidden = false;

  renderBadges(detail);
  highlightSuggestedButton(detail);

  els.metaId.textContent = detail.id;
  els.metaSource.textContent = `${detail.source} · ${detail.agent_id || ""}`.trim();
  els.metaModel.textContent = detail.model || "(unknown)";
  els.metaProject.textContent = detail.project_cwd || "";
  els.metaTurn.textContent = detail.turn_started_at || "";
  els.metaInvocation.textContent = detail.invocation_signal || "";

  els.questionBody.innerHTML = renderMarkdownish(detail.question || "");
  els.responseBody.innerHTML = renderMarkdownish(detail.response || "");

  els.conversationBody.textContent = "(loading…)";
  api.conversation(id).then((text) => {
    if (currentId === id) els.conversationBody.textContent = text || "(no conversation file)";
  });

  els.includeCheckbox.checked = !!detail.included;
  els.includeCheckbox.disabled = detail.label === "TODO";

  renderList();
}

async function showNextOrEmpty() {
  await refreshList();
  const nextId = nextUngradedId();
  if (nextId) {
    await loadCandidate(nextId);
  } else {
    currentId = null;
    currentDetail = null;
    els.card.hidden = true;
    els.emptyState.hidden = false;
  }
}

async function gradeCurrentCandidate(label) {
  if (!currentId) return;
  const id = currentId;
  const result = await api.grade(id, label);
  skippedThisRound.delete(id);
  await refreshState();
  if (result.suggestions_recomputed) {
    showToast("Heuristic suggestions recomputed for remaining candidates.");
  }
  await showNextOrEmpty();
}

function skipCurrentCandidate() {
  if (!currentId) return;
  skippedThisRound.add(currentId);
  showNextOrEmpty();
}

async function toggleInclude() {
  if (!currentId || els.includeCheckbox.disabled) return;
  const included = els.includeCheckbox.checked;
  await api.setIncluded(currentId, included);
  await refreshState();
  await refreshList();
}

/* ------------------------- wiring ------------------------- */
els.btnPass.addEventListener("click", () => gradeCurrentCandidate("pass"));
els.btnFail.addEventListener("click", () => gradeCurrentCandidate("fail"));
els.btnSkip.addEventListener("click", () => skipCurrentCandidate());
els.includeCheckbox.addEventListener("change", toggleInclude);

els.filterSource.addEventListener("change", async () => {
  skippedThisRound.clear();
  await showNextOrEmpty();
});
els.filterStatus.addEventListener("change", async () => {
  await refreshList();
  // Status filter is for browsing; grading queue always targets ungraded regardless.
  if (els.filterStatus.value && els.filterStatus.value !== "ungraded") return;
  await showNextOrEmpty();
});

els.jumpForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  const id = els.jumpInput.value.trim();
  if (!id) return;
  const detail = await api.candidate(id);
  if (!detail) {
    showToast(`No candidate with id "${id}"`);
    return;
  }
  await loadCandidate(id);
  els.jumpInput.value = "";
});

els.exportBtn.addEventListener("click", async () => {
  const ok = window.confirm(
    "Export the curated calibration set now?\n\nThis writes e2e/judge-calibration/evals/labels.yaml " +
      "from every candidate that is BOTH graded AND marked for inclusion."
  );
  if (!ok) return;
  const result = await api.exportSet();
  showToast(`Exported ${result.exported_count} candidates to ${result.path}`, 5000);
});

document.addEventListener("keydown", (e) => {
  const tag = (document.activeElement && document.activeElement.tagName) || "";
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  switch (e.key) {
    case "p":
      gradeCurrentCandidate("pass");
      break;
    case "f":
      gradeCurrentCandidate("fail");
      break;
    case "s":
    case "n":
      skipCurrentCandidate();
      break;
    case "i":
      if (!els.includeCheckbox.disabled) {
        els.includeCheckbox.checked = !els.includeCheckbox.checked;
        toggleInclude();
      }
      break;
    default:
      return;
  }
  e.preventDefault();
});

/* --------------------------- boot --------------------------- */
(async function boot() {
  await refreshState();
  await showNextOrEmpty();
  setInterval(refreshState, 15000);
})();
