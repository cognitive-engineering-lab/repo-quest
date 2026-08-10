const POLL_INTERVAL_MS = 300;
const POLL_TIMEOUT_MS = 10000;

async function questState() {
    const [chapters, current] = await Promise.all([
        getJson(`/rq/quest/${window.repoId}/chapter`),
        getJson(`/rq/quest/${window.repoId}/chapter/current`),
    ]);
    return { chapters, current };
}

// Classifies the merged pull request on this page relative to the current
// chapter of the quest. Returns null if this pull request is not the one the
// learner has just merged.
function classify({ chapters, current }) {
    if (!current) {
        return null;
    }

    const prNumber = Number(window.rqPrNumber);
    const chapterId = chapters.findIndex(
        (task) => task && task.pr.number === prNumber,
    );

    // Not a chapter pull request, e.g. a reference solution.
    if (chapterId === -1)
	return null;

    if (chapterId === current.id - 1)
	return { state: "ready", chapter: current };

    // The webhook that opens the next chapter may not have finished yet.
    if (chapterId === current.id)
	return { state: current.id + 1 < chapters.length ? "pending" : "done" };

    // An older chapter that the learner has already moved past.
    return null;
}

function renderNext(box, { state, chapter }) {
    for (const body of box.querySelectorAll("[data-rq-next-state]")) {
        body.hidden = body.dataset.rqNextState !== state;
    }

    if (state !== "ready") return;

    const link = box.querySelector("a.rq-next-issue-link");
    const name = box.querySelector(".rq-next-name");
    if (!link || !name) throw new Error("Internal error: missing link or name in template");

    const issue = chapter.task.issue;
    link.href = `${window.appSubUrl}/${issue.owner}/${issue.repo}/issues/${issue.number}`;
    name.textContent = chapter.taskName;
}

async function insertNext() {
    const container = document.querySelector(".page-content.repository");
    const template = document.getElementById("rq-next-template");
    if (!container || !template) throw new Error("Missing elements");

    let status = classify(await questState());
    if (!status) return;

    const box = template.content.firstElementChild.cloneNode(true);
    container.prepend(box);
    renderNext(box, status);

    // Only reached when this page's pull request is the current chapter's, i.e.
    // the quest has not advanced past it yet.
    const advancing = status.state === "pending";

    const deadline = Date.now() + POLL_TIMEOUT_MS;
    while (status.state === "pending" && Date.now() < deadline) {
        await sleep(POLL_INTERVAL_MS);
        status = classify(await questState()) ?? status;
        renderNext(box, status);
    }

    if (status.state === "pending") {
        renderNext(box, { state: "timeout" });
        return;
    }

    // The sidebar was rendered against the chapter we just finished.
    if (advancing && status.state === "ready") {
        reportErrors(renderSidebar());
    }
}

insertNext();
