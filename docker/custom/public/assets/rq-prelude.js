function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

async function fetchJson(path, method) {
    const response = await fetch(`${window.appSubUrl}${path}`, {
        method,
        headers: { "Content-Type": "application/json" },
    });
    const body = await response.text();
    if (!response.ok) {
        throw new Error(
            `Error from ${method} ${path}: ${response.status} ${body}`,
        );
    }
    // Endpoints that return no data at all, such as /rq/error/clear, send an
    // empty body rather than "null".
    return body ? JSON.parse(body) : null;
}

function getJson(path) {
    return fetchJson(path, "GET");
}

function postJson(path) {
    return fetchJson(path, "POST");
}
