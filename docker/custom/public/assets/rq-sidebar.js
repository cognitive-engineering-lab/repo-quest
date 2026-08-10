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
}

function showError(msg) {
    const notice = document.getElementById("rq-error-notice");
    const list = document.getElementById("rq-error-list");

    var entry = document.createElement("li");
    entry.appendChild(document.createTextNode(msg));
    list.appendChild(entry);

    notice.style.display = "block";

    throw new Error(msg);
}

function reportErrors(promise) {
    promise.catch((err) => showError(err.message));
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
            if (errors.length > 0) {
                showError(
                    `Server has encountered errors: ${errors[errors.length - 1]}`,
                );
            }
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

async function loadCurrentQuest() {
    const quest = await getJson(`/rq/quest/${window.repoId}`);

    const currentQuestSpan = document.getElementById("current-quest-title");
    currentQuestSpan.innerHTML = quest.title;
}

async function loadCurrentChapter() {
    const chapter = await getJson(
        `/rq/quest/${window.repoId}/chapter/current`,
    );

    const currentIssueLink = document.getElementById("current-task-link");
    const currentPrLink = document.getElementById("current-pr-link");
    const curTaskNameSpan = document.getElementById("current-task-name");
    const curBranchNameLink = document.getElementById("current-branch-name");
    if (!chapter) {
        currentPrLink.remove();
        curTaskNameSpan.remove();
	curBranchNameSpan.remove();
        currentIssueLink.remove();
    } else {
        const issue = chapter.task.issue;
        currentIssueLink.href = `${window.appSubUrl}/${issue.owner}/${issue.repo}/issues/${issue.number}`;

	const pr = chapter.task.pr;
        currentPrLink.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;

        curTaskNameSpan.innerHTML = chapter.taskName;
	curBranchNameLink.innerHTML = chapter.branchName;
	curBranchNameLink.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/src/branch/${chapter.branchName}`;
    }
}

async function createSolution() {
    const pr = await postJson(
        `/rq/quest/${window.repoId}/chapter/current/reference_solution`,
    );
    window.location.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;
}

async function loadReferenceSolution() {
    const pr = await getJson(
        `/rq/quest/${window.repoId}/chapter/current/reference_solution`,
    );

    const solnButtonSpan = document.getElementById("solution-button-span");
    if (!pr) {
        const button = document.createElement("a");
        button.type = "button";
        button.classList.add("ui");
        button.classList.add("fluid");
        button.classList.add("button");
        button.addEventListener("click", () => reportErrors(createSolution()));
        button.append("Add Solution PR");
        solnButtonSpan.replaceWith(button);
    } else {
        const link = document.createElement("a");
        link.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;
        link.classList.add("text");
        link.classList.add("flex-text-block");
        link.append("Pull Request");
        solnButtonSpan.replaceWith(link);
    }
}

insertSidebar();
listenForErrors();
reportErrors(loadCurrentQuest());
reportErrors(loadCurrentChapter());
reportErrors(loadReferenceSolution());
