const introKey = `rq_intro_collapsed_${window.repoId}`;

function introCollapsed() {
    return localStorage.getItem(introKey) !== null;
}

function setIntroCollapsed(collapsed) {
    if (collapsed) {
	localStorage.setItem(introKey, "1");
    } else {
	localStorage.removeItem(introKey);
    }
}

function cloneUrl() {
    const input = document.getElementById("repo-clone-url");
    if (input) {
	return input.value;
    }
    const el = document.querySelector(".js-clone-url:not(.rq-intro-clone-url)");
    if (el) {
	return el.nodeName === "INPUT" ? el.value : el.textContent;
    }
    return null;
}

function fillCloneCommand(intro) {
    const target = intro.querySelector(".rq-intro-clone-url");
    if (!target) {
	throw new Error("Internal error: missing clone url span in template");
    }

    const url = cloneUrl();
    if (url) {
	console.log("FUCK", url);
	target.textContent = url;
    } else {
	target.closest("pre").remove();
    }
}

async function loadIntroIssueLink(intro) {
    const link = intro.querySelector("a.rq-intro-issue-link");
    if (!link) {
	throw new Error("Internal error: missing link in template");
    }

    const response = await fetch(
	`${window.appSubUrl}/rq/quest/${window.repoId}/chapter/current`,
	{
	    method: "GET",
	    headers: { "Content-Type": "application/json" },
	},
    );
    if (!response.ok) {
	throw new Error(
	    `Error getting current quest chapter: ${response.status} ${response.body}`,
	);
    }

    const chapter = await response.json();
    const issue = chapter.task.issue;
    link.href = `${window.appSubUrl}/${issue.owner}/${issue.repo}/issues/${issue.number}`;
}

function insertIntro() {
    const container = document.querySelector(
	".page-content.repository",
    );
    const template = document.getElementById("rq-intro-template");
    if (!container || !template) {
	return;
    }

    const intro = template.content.firstElementChild.cloneNode(true);
    container.prepend(intro);

    const params = new URLSearchParams(window.location.search);
    intro.open = params.has("rq_intro") && !introCollapsed();
    intro.addEventListener("toggle", () => setIntroCollapsed(!intro.open));

    fillCloneCommand(intro);
    loadIntroIssueLink(intro);
}

insertIntro();
