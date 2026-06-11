const sidebar = document.createElement("div");
sidebar.innerHTML = window.sidebarHtml;

const container = document.createElement("div");
container.id = "rq-page-container";

const contentNode = document.querySelector(".page-content");
contentNode.replaceWith(container);

container.appendChild(contentNode);
container.appendChild(sidebar);

function showError(msg) {
	const notice = document.getElementById("rq-error-notice");
	const list = document.getElementById("rq-error-list");

	var entry = document.createElement("li");
	entry.appendChild(document.createTextNode(msg));
	list.appendChild(entry);

	notice.style.display = "block";

	throw new Error(msg);
}

async function clearErrors() {
	const notice = document.getElementById("rq-error-notice");
	notice.style.display = "none";

	const response = await fetch(`${window.appSubUrl}/rq/error/clear`, {
		method: "POST",
	});
	if (!response.ok) {
		showError(
			`Error getting clearing errors: ${response.status} ${response.body}`,
		);
	}
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

async function loadCurrentChapter() {
	const response = await fetch(
		`${window.appSubUrl}/rq/quest/${window.repoId}/chapter/current`,
		{
			method: "GET",
			headers: { "Content-Type": "application/json" },
		},
	);
	if (!response.ok) {
		showError(
			`Error getting current quest chapter: ${response.status} ${response.body}`,
		);
	}
	const chapter = await response.json();

	const currentIssueLink = document.getElementById("current-task-link");
	if (!chapter) {
		currentIssueLink.remove();
	} else {
		const issue = chapter.task.issue;
		currentIssueLink.href = `${window.appSubUrl}/${issue.owner}/${issue.repo}/issues/${issue.number}`;
		/* currentIssueLink.content = issue.title; */
	}

	const currentPrLink = document.getElementById("current-pr-link");
	if (!chapter) {
		currentPrLink.remove();
	} else {
		const pr = chapter.task.pr;
		currentPrLink.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;
		/* currentPrLink.content = pr.title; */
	}
	const curTaskNameSpan = document.getElementById("current-task-name");
	if (!chapter) {
		curTaskNameSpan.remove();
	} else {
		curTaskNameSpan.innerHTML = chapter.taskName;
	}
}

async function createSolution(event) {
	const response = await fetch(
		`${window.appSubUrl}/rq/quest/${window.repoId}/chapter/current/reference_solution`,
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
		},
	);
	if (!response.ok) {
		showError(`Error starting quest: ${response.status} ${response.body}`);
	}
	const pr = await response.json();
	window.location.href = `${window.appSubUrl}/${pr.owner}/${pr.repo}/pulls/${pr.number}`;
}

async function loadReferenceSolution() {
	const response = await fetch(
		`${window.appSubUrl}/rq/quest/${window.repoId}/chapter/current/reference_solution`,
		{
			method: "GET",
			headers: { "Content-Type": "application/json" },
		},
	);
	if (!response.ok) {
		showError(
			`Error checking reference solution status: ${response.status} ${response.body}`,
		);
	}
	const pr = await response.json();

	const solnButtonSpan = document.getElementById("solution-button-span");
	if (!pr) {
		const button = document.createElement("a");
		button.type = "button";
		button.classList.add("ui");
		button.classList.add("fluid");
		button.classList.add("button");
		button.addEventListener("click", createSolution);
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

listenForErrors();
loadCurrentChapter();
loadReferenceSolution();
