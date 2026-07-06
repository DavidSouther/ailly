"use strict";

/* ---------- tiny dependency-free markdown-ish renderer (reused from v1) ---------- */
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
  async judges() {
    return (await fetch("/api/judges")).json();
  },
  async judge(judgeId) {
    const res = await fetch(`/api/judge?judge_id=${encodeURIComponent(judgeId)}`);
    if (!res.ok) return null;
    return res.json();
  },
  async cell(judgeId, candidateId) {
    const res = await fetch(
      `/api/cell?judge_id=${encodeURIComponent(judgeId)}&candidate_id=${encodeURIComponent(candidateId)}`
    );
    if (!res.ok) return null;
    return res.json();
  },
  async grade(judgeId, candidateId, verdict) {
    const res = await fetch("/api/grade", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ judge_id: judgeId, candidate_id: candidateId, verdict }),
    });
    return res.json();
  },
};

/* --------------------------- elements --------------------------- */
const els = {
  progress: document.getElementById("progress"),
  backBtn: document.getElementById("back-btn"),
  judgePicker: document.getElementById("judge-picker"),
  pickableList: document.getElementById("pickable-list"),
  doneSection: document.getElementById("done-section"),
  doneList: document.getElementById("done-list"),
  noDataState: document.getElementById("no-data-state"),

  layout: document.getElementById("layout"),
  judgeId: document.getElementById("judge-id"),
  judgeTags: document.getElementById("judge-tags"),
  rubricBody: document.getElementById("rubric-body"),
  cellList: document.getElementById("cell-list"),

  emptyState: document.getElementById("empty-state"),
  cellCard: document.getElementById("cell-card"),
  metaCandidate: document.getElementById("meta-candidate"),
  metaSource: document.getElementById("meta-source"),
  metaProject: document.getElementById("meta-project"),
  metaMatched: document.getElementById("meta-matched"),
  userBody: document.getElementById("user-body"),
  assistantBody: document.getElementById("assistant-body"),

  btnPass: document.getElementById("btn-pass"),
  btnFail: document.getElementById("btn-fail"),
  btnInconclusive: document.getElementById("btn-inconclusive"),
  toast: document.getElementById("toast"),
};

let currentJudgeId = null;
let currentJudgeDetail = null; // {judge_id, prompt, cells: [...]}
let currentCandidateId = null;
const skippedThisRound = new Set(); // candidate ids marked Inconclusive this round, for the current judge
let toastTimer = null;

function showToast(msg, ms = 3000) {
  els.toast.textContent = msg;
  els.toast.hidden = false;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    els.toast.hidden = true;
  }, ms);
}

/* --------------------------- judge picker --------------------------- */
async function refreshJudgePicker() {
  const { judges } = await api.judges();
  const pickable = judges.filter((j) => j.pickable);
  const done = judges.filter((j) => !j.pickable);

  els.progress.textContent = `${judges.length} judge${judges.length === 1 ? "" : "s"} with relevant cells · ${pickable.length} ungraded`;

  els.pickableList.innerHTML = "";
  for (const j of pickable) {
    els.pickableList.appendChild(judgeListItem(j));
  }
  els.doneList.innerHTML = "";
  for (const j of done) {
    els.doneList.appendChild(judgeListItem(j));
  }
  els.doneSection.hidden = done.length === 0;
  els.noDataState.hidden = judges.length !== 0;
  return judges;
}

function judgeListItem(j) {
  const li = document.createElement("li");
  li.className = "judge-card" + (j.pickable ? "" : " done");
  li.innerHTML = `
    <div class="judge-card-id">${j.judge_id}</div>
    <div class="judge-card-meta">${j.suite}${j.case_name ? " · " + j.case_name : ""}</div>
    <div class="judge-card-progress">${j.graded_cells} / ${j.total_cells} graded${j.pickable ? ` — ${j.remaining_cells} remaining` : " — done"}</div>
  `;
  li.addEventListener("click", () => openJudge(j.judge_id));
  return li;
}

function showPicker() {
  currentJudgeId = null;
  currentJudgeDetail = null;
  currentCandidateId = null;
  skippedThisRound.clear();
  els.judgePicker.hidden = false;
  els.layout.hidden = true;
  els.backBtn.hidden = true;
  refreshJudgePicker();
}

/* --------------------------- judge + cell review --------------------------- */
async function openJudge(judgeId) {
  const detail = await api.judge(judgeId);
  if (!detail) {
    showToast(`Unknown judge id: ${judgeId}`);
    return;
  }
  currentJudgeId = judgeId;
  currentJudgeDetail = detail;
  skippedThisRound.clear();

  els.judgePicker.hidden = true;
  els.layout.hidden = false;
  els.backBtn.hidden = false;

  els.judgeId.textContent = detail.judge_id;
  els.judgeTags.innerHTML = `
    <span class="tag">${detail.suite}</span>
    ${detail.case_name ? `<span class="tag">${detail.case_name}</span>` : ""}
    ${detail.needs_human_review ? '<span class="tag warn">needs human review</span>' : ""}
  `;
  els.rubricBody.innerHTML = renderMarkdownish(detail.prompt || "");

  renderCellList();
  await showNextOrEmpty();
}

function renderCellList() {
  els.cellList.innerHTML = "";
  for (const c of currentJudgeDetail.cells) {
    const li = document.createElement("li");
    li.dataset.id = c.candidate_id;
    if (c.candidate_id === currentCandidateId) li.classList.add("active");
    const chipClass = c.verdict === "Pass" ? "pass" : c.verdict === "Fail" ? "fail" : "todo";
    const chipText = c.verdict || "TODO";
    li.innerHTML = `
      <div class="row1">
        <span class="chip ${chipClass}">${chipText}</span>
        <span>${c.source || ""}</span>
      </div>
      <div class="cid">${c.candidate_id}</div>
    `;
    li.addEventListener("click", () => loadCell(c.candidate_id));
    els.cellList.appendChild(li);
  }
}

function updateProgressLine() {
  const total = currentJudgeDetail.cells.length;
  const graded = currentJudgeDetail.cells.filter((c) => c.verdict).length;
  els.progress.textContent = `${currentJudgeDetail.judge_id} · ${graded} / ${total} graded`;
}

function nextUngradedCandidateId() {
  const pool = currentJudgeDetail.cells.filter(
    (c) => !c.verdict && !skippedThisRound.has(c.candidate_id)
  );
  if (pool.length === 0) {
    if (skippedThisRound.size > 0) {
      // Everything left was marked Inconclusive this round; give it another lap.
      skippedThisRound.clear();
      return nextUngradedCandidateId();
    }
    return null;
  }
  return pool[0].candidate_id;
}

async function loadCell(candidateId) {
  const detail = await api.cell(currentJudgeId, candidateId);
  if (!detail) {
    showToast(`Unknown candidate id for this judge: ${candidateId}`);
    return;
  }
  currentCandidateId = candidateId;
  els.emptyState.hidden = true;
  els.cellCard.hidden = false;

  els.metaCandidate.textContent = detail.cell.candidate_id;
  els.metaSource.textContent = `${detail.cell.source || ""} · ${detail.model || ""}`.trim();
  els.metaProject.textContent = detail.cell.project_cwd || "";
  els.metaMatched.textContent = `${detail.cell.matched_keyword || ""} (${detail.cell.matched_tag || ""})`;

  els.userBody.innerHTML = renderMarkdownish(detail.user || "");
  els.assistantBody.innerHTML = renderMarkdownish(detail.assistant || "");

  renderCellList();
  updateProgressLine();
}

async function showNextOrEmpty() {
  updateProgressLine();
  const nextId = nextUngradedCandidateId();
  if (nextId) {
    await loadCell(nextId);
  } else {
    currentCandidateId = null;
    els.cellCard.hidden = true;
    els.emptyState.hidden = false;
    renderCellList();
  }
}

async function gradeCurrentCell(verdict) {
  if (!currentCandidateId) return;
  const candidateId = currentCandidateId;
  const result = await api.grade(currentJudgeId, candidateId, verdict);
  if (result.error) {
    showToast(result.error);
    return;
  }
  skippedThisRound.delete(candidateId);
  const cell = currentJudgeDetail.cells.find((c) => c.candidate_id === candidateId);
  if (cell) cell.verdict = result.verdict;
  await showNextOrEmpty();
}

function markInconclusive() {
  // Skip with no write -- functionally identical to v1's Skip.
  if (!currentCandidateId) return;
  skippedThisRound.add(currentCandidateId);
  showNextOrEmpty();
}

/* ------------------------- wiring ------------------------- */
els.backBtn.addEventListener("click", showPicker);
els.btnPass.addEventListener("click", () => gradeCurrentCell("pass"));
els.btnFail.addEventListener("click", () => gradeCurrentCell("fail"));
els.btnInconclusive.addEventListener("click", markInconclusive);

document.addEventListener("keydown", (e) => {
  const tag = (document.activeElement && document.activeElement.tagName) || "";
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  if (els.layout.hidden) return; // shortcuts only apply while reviewing a judge's cells
  switch (e.key) {
    case "p":
      gradeCurrentCell("pass");
      break;
    case "f":
      gradeCurrentCell("fail");
      break;
    case "i":
      markInconclusive();
      break;
    default:
      return;
  }
  e.preventDefault();
});

/* --------------------------- boot --------------------------- */
(async function boot() {
  showPicker();
})();
