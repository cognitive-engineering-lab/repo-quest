function insertSidebar() {
    const contentNode = document.querySelector(".page-content");
    const template = document.getElementById("rq-sidebar-template");
    if (!contentNode || !template) {
        return;
    }

    const sidebar = template.content.firstElementChild.cloneNode(true);

    const container = document.createElement("div");
    container.id = "rq-page-container";
    contentNode.replaceWith(container);

    container.appendChild(contentNode);
    container.appendChild(sidebar);

    sidebar
        .querySelector("#rq-error-clear")
        .addEventListener("click", () => reportErrors(clearErrors()));

    // Attached once here rather than on each render, which only repopulates
    // the options.
    sidebar.querySelector("#rq-skip-select").addEventListener("change", onSkipSelect);
}

function showError(msg) {
    const notice = document.getElementById("rq-error-notice");
    const list = document.getElementById("rq-error-list");

    var entry = document.createElement("li");
    entry.appendChild(document.createTextNode(msg));
    list.appendChild(entry);

    notice.style.display = "block";

    // NOTE(wc): for now, let's not rethrow these errors and only handle them within RQ.
    // Prevents an excessive # of red boxes from showing on screen.
    // throw new Error(msg);
}

function reportErrors(promise) {
    promise.catch((err) => showError(`Client error: ${err.message}`));
}

async function clearErrors() {
    const notice = document.getElementById("rq-error-notice");
    notice.style.display = "none";

    await postJson("/rq/error/clear");
}

function listenForErrors() {
    let websocket = null;

    function listen() {
        websocket = new WebSocket(`${window.appSubUrl}/rq/error/listen`);
        websocket.addEventListener("message", (e) => {
            let errors = JSON.parse(e.data);
	    if (errors.length === 0) return;
	    let error = errors[errors.length - 1];
	    showError(`Server error: ${error}`);
        });
    }

    // For why we explicitly close and restart the socket, see
    // https://developer.mozilla.org/en-US/docs/Web/API/WebSockets_API/Writing_WebSocket_client_applications#working_with_the_bfcache
    window.addEventListener("pagehide", () => {
        if (websocket) {
            websocket.close();
            websocket = null;
            const list = document.getElementById("rq-error-list");
            list.innerHTML = "";
        }
    });

    window.addEventListener("pageshow", (event) => {
        if (event.persisted) {
            listen();
        }
    });

    // the server always responds with the current state of errors, so no need
    // for a separate check.
    listen();
}

function renderCurrentQuest(quest) {
    const currentQuestLink = document.getElementById("current-quest-link");
    currentQuestLink.innerHTML = quest.title;
    currentQuestLink.href = quest.repository;

    const questBugLink = document.getElementById("quest-bug-link");
    questBugLink.href = quest.repository;
}

function renderCurrentChapter(chapter) {
    const currentIssueLink = document.getElementById("current-task-link");
    const currentPrLink = document.getElementById("current-pr-link");
    const curTaskNameSpan = document.getElementById("current-task-name");
    const curBranchNameLink = document.getElementById("current-branch-name");

    // Hidden rather than removed so that the sidebar can be re-rendered.
    for (const el of [
        currentIssueLink,
        currentPrLink,
        curTaskNameSpan,
        curBranchNameLink,
    ]) {
        el.hidden = !chapter;
    }
    if (!chapter) {
        return;
    }

    const issue = chapter.task.issue;
    currentIssueLink.href = `${window.appSubUrl}/${issue.owner}/${issue.repo}/issues/${issue.number}`;

    const pr = chapter.task.pr;
    currentPrLink.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;

    curTaskNameSpan.innerHTML = chapter.taskName;
    curBranchNameLink.innerHTML = chapter.branchName;
    curBranchNameLink.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/src/branch/${chapter.branchName}`;
}

async function createSolution() {
    const pr = await postJson(
        `/rq/quest/${window.repoId}/chapter/current/reference_solution`,
    );
    window.location.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;
}

function renderReferenceSolution(pr) {
    // Filled rather than replaced so that the sidebar can be re-rendered.
    const solnButton = document.getElementById("solution-button");
    if (!pr) {
        const button = document.createElement("a");
        button.type = "button";
        button.classList.add("ui");
        button.classList.add("fluid");
        button.classList.add("button");
        button.addEventListener("click", () => reportErrors(createSolution()));
        button.append("Add Solution PR");
        solnButton.replaceChildren(button);
    } else {
        const link = document.createElement("a");
        link.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;
        link.classList.add("text");
        link.classList.add("flex-text-block");
        link.append("Reference solution pull request");
        solnButton.replaceChildren(link);
    }
}

// Lists the chapters that the quest can be skipped ahead to. Chapters at or
// before the current one are shown but disabled, since skipping is forward-only.
function renderQuestTools(quest, chapter) {
    const select = document.getElementById("rq-skip-select");
    const current = chapter ? chapter.id : null;

    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "Skip to chapter…";

    const options = quest.tasks.map((task, index) => {
        const option = document.createElement("option");
        option.value = index;
        option.textContent = `${index}. ${task.issueTemplate.title}`;
        if (current !== null && index <= current) {
            option.disabled = true;
            if (index === current) {
                option.textContent += " (current)";
            }
        }
        return option;
    });

    select.replaceChildren(placeholder, ...options);
}

async function skipToChapter(chapterNumber) {
    const task = await postJson(
        `/rq/quest/${window.repoId}/skip/${chapterNumber}`,
    );
    const issue = task.issue;
    window.location.href = `${window.appSubUrl}/${issue.owner}/${issue.repo}/issues/${issue.number}`;
}

function onSkipSelect(event) {
    const select = event.target;
    if (select.value === "") {
        return;
    }

    const chapterNumber = Number(select.value);
    const title = select.options[select.selectedIndex].textContent;
    select.selectedIndex = 0;

    const confirmed = window.confirm(
        `Skip ahead to "${title}"?\n\n` +
	    "This rewrites your repository: the main branch will be reset to the " +
	    "reference solution for the preceding chapter, and any work you have " +
	    "not merged will be lost. The chapters you skip cannot be returned to.\n\n" +
	    "If you have a local clone, you will need to run:\n" +
	    "    git fetch origin && git reset --hard origin/main",
    );
    if (!confirmed) {
        return;
    }

    reportErrors(skipToChapter(chapterNumber));
}

// Repopulates the sidebar from the server. Safe to call repeatedly, so other
// components can call it after an action that invalidates what it shows.
async function renderSidebar() {
    const [quest, chapter, referenceSolution] = await Promise.all([
        getJson(`/rq/quest/${window.repoId}`),
        getJson(`/rq/quest/${window.repoId}/chapter/current`),
        getJson(`/rq/quest/${window.repoId}/chapter/current/reference_solution`),
    ]);

    renderCurrentQuest(quest);
    renderCurrentChapter(chapter);
    renderReferenceSolution(referenceSolution);
    renderQuestTools(quest, chapter);
}

insertSidebar();
listenForErrors();
reportErrors(renderSidebar());
