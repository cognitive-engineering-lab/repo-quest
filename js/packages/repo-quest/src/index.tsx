import * as dialog from "@tauri-apps/plugin-dialog";
import { type Quiz, QuizView } from "@wcrichto/quiz";
import _ from "lodash";
import { marked } from "marked";
import { makeAutoObservable, runInAction } from "mobx";
import { observer } from "mobx-react";
import React, { useContext, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import ReactDOM from "react-dom/client";

import guideMd from "../../../../GUIDE.md?raw";
import {
  events,
  type QuestConfig,
  type QuestLocation,
  type QuestState,
  type Result,
  type Stage,
  type StageState,
  type StateDescriptor,
  commands
} from "./bindings/backend";

let useWindowListener = <K extends keyof WindowEventMap>(
  event: K,
  listener: (this: Window, ev: WindowEventMap[K]) => void
) => {
  useEffect(() => {
    window.addEventListener(event, listener);
    return () => window.removeEventListener(event, listener);
  }, []);
};

let Link: React.FC<React.AnchorHTMLAttributes<HTMLAnchorElement>> = props => (
  <a target="_blank" {...props} />
);

interface AwaitProps<T> {
  promise: Promise<T> | (() => Promise<T>);
  children: (t: T) => React.ReactNode;
}

class Loader {
  loading = false;
  static context = React.createContext<Loader | undefined>(undefined);

  constructor() {
    makeAutoObservable(this);
  }

  View = observer(() =>
    this.loading
      ? createPortal(
          <div id="loading-cover">
            <div id="spinner" />
          </div>,
          document.body
        )
      : null
  );

  loadAwait = async <T,>(promise: Promise<T>) => {
    runInAction(() => {
      this.loading = true;
    });
    try {
      let value = await promise;
      return value;
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  };
}

function Await<T>(props: AwaitProps<T>) {
  let loader = useContext(Loader.context)!;
  let [value, setValue] = useState<T | undefined>(undefined);
  useEffect(() => {
    let promise =
      typeof props.promise === "function" ? props.promise() : props.promise;
    loader.loadAwait(promise.then(setValue));
  }, []);

  return value !== undefined && props.children(value);
}

let TitleContext = React.createContext<((title: string) => void) | undefined>(
  undefined
);

interface ErrorMessage {
  action: string;
  message: string;
}

let ErrorContext = React.createContext<
  ((message: ErrorMessage) => void) | undefined
>(undefined);

let ErrorView: React.FC<{ message: string; action: string }> = ({
  message,
  action
}) => {
  let setMessage = useContext(ErrorContext)!;
  useEffect(() => setMessage({ message, action }), [message, action]);
  return null;
};

async function tryAwait<T>(
  promise: Promise<Result<T, string>>,
  action: string,
  setMessage: (message: ErrorMessage) => void
) {
  let result = await promise;
  if (result.status === "error") {
    setMessage({ action, message: result.error });
  }
}

let GithubLoader = () => (
  <Await promise={commands.getGithubToken()}>
    {token =>
      token.type === "Found" ? (
        <Await promise={commands.initOctocrab(token.value)}>
          {result =>
            result.status === "ok" ? (
              <LoaderEntry />
            ) : (
              <ErrorView action="Loading Github API" message={result.error} />
            )
          }
        </Await>
      ) : token.type === "NotFound" ? (
        <>
          <div>
            Before running RepoQuest, you need to provide it access to Github.
            Follow the instructions at the link below and restart RepoQuest.
          </div>
          <div>
            <Link href="https://github.com/cognitive-engineering-lab/repo-quest/blob/main/README.md#github-token">
              https://github.com/cognitive-engineering-lab/repo-quest/blob/main/README.md#github-token
            </Link>
          </div>
        </>
      ) : (
        <ErrorView action="Loading Github token" message={token.value} />
      )
    }
  </Await>
);

let LoaderEntry = () => {
  let promise = async () => {
    let cwd = await commands.currentDir();
    return await commands.loadQuest(cwd);
  };
  return (
    <Await promise={promise}>
      {quest_res =>
        quest_res.status === "ok" ? (
          <QuestView
            quest={quest_res.data[0]}
            initialState={quest_res.data[1]}
          />
        ) : (
          <InitForm />
        )
      }
    </Await>
  );
};

let InitForm = () => {
  type InitState = { type: "new" } | { type: "load"; dir: string } | undefined;
  let [selected, setSelected] = useState<InitState>(undefined);
  return selected === undefined ? (
    <div className="controls">
      <button type="button" onClick={() => setSelected({ type: "new" })}>
        Start a new quest
      </button>

      <button
        type="button"
        onClick={async () => {
          let dir = await dialog.open({ directory: true });
          if (dir !== null) setSelected({ type: "load", dir });
        }}
      >
        Load an existing quest
      </button>
    </div>
  ) : selected.type === "new" ? (
    <NewQuest />
  ) : (
    <Await promise={commands.loadQuest(selected.dir)}>
      {quest_res =>
        quest_res.status === "ok" ? (
          <QuestView
            quest={quest_res.data[0]}
            initialState={quest_res.data[1]}
          />
        ) : (
          <ErrorView action="Creating new quest" message={quest_res.error} />
        )
      }
    </Await>
  );
};

const QUESTS = ["cognitive-engineering-lab/rqst-async"];

let NewQuest = () => {
  let [dir, setDir] = useState<string | undefined>(undefined);
  let [quest, setQuest] = useState<QuestLocation | undefined>(undefined);
  let [submit, setSubmit] = useState(false);
  return !submit ? (
    <div className="new-quest">
      <div>
        <strong>Start a new quest</strong>
      </div>
      <table>
        <tbody>
          <tr>
            <td>Quest:</td>
            <td>
              <select
                onChange={e =>
                  setQuest({ type: "Remote", value: e.target.value })
                }
                defaultValue={""}
              >
                <option disabled={true} value="">
                  Choose a quest
                </option>
                {QUESTS.map(quest => (
                  <option
                    key={quest}
                    value="cognitive-engineering-lab/rqst-async"
                  >
                    {quest.split("/")[1]}
                  </option>
                ))}
              </select>

              <br />
              <span className="separator">or</span>

              <input
                type="text"
                placeholder="Enter a GitHub repo like foo/bar"
                onChange={e => {
                  if (e.target.checkValidity())
                    setQuest({ type: "Remote", value: e.target.value });
                  else setQuest(undefined);
                }}
                pattern="[^\/]+\/.+"
              />

              <br />
              <span className="separator">or</span>

              <button
                className="file-picker"
                type="button"
                onClick={async () => {
                  let file = await dialog.open();
                  if (file !== null) setQuest({ type: "Local", value: file });
                }}
              >
                Choose a local package file
              </button>
              {quest && quest.type === "Local" && <code>{quest.value}</code>}
            </td>
          </tr>
          <tr>
            <td>Directory:</td>
            <td>
              <button
                className="file-picker"
                type="button"
                onClick={async () => {
                  let dir = await dialog.open({ directory: true });
                  if (dir !== null) setDir(dir);
                }}
              >
                Choose a dir
              </button>
              {dir && <code>{dir}</code>}
            </td>
          </tr>
        </tbody>
      </table>
      <div>
        <button
          type="button"
          disabled={dir === undefined || quest === undefined}
          onClick={() => setSubmit(true)}
        >
          Create
        </button>
      </div>
    </div>
  ) : (
    <Await promise={commands.newQuest(dir!, quest!)}>
      {quest_res =>
        quest_res.status === "ok" ? (
          <QuestView
            quest={quest_res.data[0]}
            initialState={quest_res.data[1]}
          />
        ) : (
          <ErrorView action="Creating new quest" message={quest_res.error} />
        )
      }
    </Await>
  );
};

let QuizPage: React.FC<{ quest: QuestConfig }> = ({ quest }) => {
  let quiz = quest.final as
    /* biome-ignore lint/suspicious/noExplicitAny: backend guarantees that this satisfies Quiz */
    any as Quiz;
  return (
    <QuizView
      name={quest.title}
      quiz={quiz}
      cacheAnswers={true}
      autoStart={true}
      allowRetry={true}
    />
  );
};

let QuestView: React.FC<{
  quest: QuestConfig;
  initialState: StateDescriptor;
}> = ({ quest, initialState }) => {
  console.debug(quest);

  let loader = useContext(Loader.context)!;
  let [state, setState] = useState<StateDescriptor | undefined>(initialState);
  let [showQuiz, setShowQuiz] = useState(false);
  let setTitle = useContext(TitleContext)!;
  useEffect(() => setTitle(quest.title), [quest.title]);

  useEffect(() => {
    events.stateEvent.listen(e => setState(e.payload));
  }, []);

  let cur_stage =
    state && state.state.type === "Ongoing"
      ? state.state.stage
      : quest.stages.length - 1;

  if (showQuiz) {
    return <QuizPage quest={quest} />;
  }

  return (
    <div className="columns">
      <div>
        {state !== undefined && (
          <>
            <div className="quest-dir">
              <strong>Quest directory:</strong> <code>{state.dir}</code>
            </div>
            {state.behind_origin && (
              <div className="behind-origin-warning">
                Your local repo is not up-to-date with the Github repo, run{" "}
                <code>git pull</code>!
              </div>
            )}
            <ol className="stages" start={0}>
              {_.range(cur_stage + 1).map(i => (
                <StageView
                  key={i}
                  index={i}
                  stage={state.stages[i]}
                  state={state.state}
                />
              ))}
              {state.state.type === "Completed" && quest.final && (
                <li>
                  <div>
                    <span className="stage-title">Quiz</span>
                  </div>
                  <div>
                    <button type="button" onClick={() => setShowQuiz(true)}>
                      Start
                    </button>
                  </div>
                </li>
              )}
            </ol>
          </>
        )}
      </div>
      <div className="meta">
        <h2>Controls</h2>
        <div>
          <div>
            <button
              type="button"
              onClick={() => loader.loadAwait(commands.refreshState())}
            >
              Refresh UI state
            </button>
          </div>

          <div>
            <button
              type="button"
              onClick={() => {
                if (state) navigator.clipboard.writeText(state.dir);
              }}
            >
              Copy directory to 📋
            </button>
          </div>

          {initialState.can_skip && (
            <div>
              <select
                defaultValue={""}
                onChange={async e => {
                  if (e.target.value === "") return;
                  let confirmed = await dialog.confirm(
                    "This will irrevocably overwrite any changes you've made. Are you sure?"
                  );
                  let stage = Number.parseInt(e.target.value);
                  e.target.value = "";
                  if (confirmed)
                    await loader.loadAwait(commands.skipToStage(stage));
                }}
              >
                <option disabled={true} value="">
                  Skip to chapter...
                </option>
                {quest.stages
                  .map<[Stage, number]>((stage, i) => [stage, i])
                  .filter(([_stage, i]) => i > cur_stage)
                  .map(([stage, i]) => (
                    <option key={stage.label} value={i}>
                      Chapter {i}: {stage.name}
                    </option>
                  ))}
              </select>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

let StageView: React.FC<{
  index: number;
  stage: StageState;
  state: QuestState;
}> = ({ index, stage, state }) => {
  let loader = useContext(Loader.context)!;
  let setMessage = useContext(ErrorContext)!;
  return (
    <li>
      <div>
        <span className="stage-title">{stage.stage.name}</span>
        <span className="separator">·</span>
        {state.type === "Ongoing" && state.stage === index ? (
          state.part === "Starter" ? (
            state.status === "Start" ? (
              <button
                type="button"
                onClick={() =>
                  loader.loadAwait(
                    tryAwait(
                      commands.fileFeatureAndIssue(index),
                      "Filing issue or feature PR",
                      setMessage
                    )
                  )
                }
              >
                {stage.stage["no-starter"]
                  ? "File issue"
                  : "File issue & starter PR"}
              </button>
            ) : (
              <span className="status">
                Waiting for you to merge starter PR
              </span>
            )
          ) : state.status === "Start" ? (
            stage.reference_solution_pr_url ? (
              <details className="help">
                <summary>Help</summary>
                <div>
                  Try first learning from our reference solution and
                  incorporating it into your codebase. If that doesn't work, we
                  can replace your code with ours.
                </div>
                <div>
                  <div>
                    <Link href={stage.reference_solution_pr_url}>
                      View reference solution
                    </Link>
                  </div>
                  <div>
                    <button
                      type="button"
                      onClick={() =>
                        loader.loadAwait(
                          tryAwait(
                            commands.fileSolution(index),
                            "Filing solution PR",
                            setMessage
                          )
                        )
                      }
                    >
                      File reference solution
                    </button>
                  </div>
                </div>
              </details>
            ) : (
              <span className="status">
                Waiting for you to solve and close issue
              </span>
            )
          ) : (
            <span className="status">
              Waiting for you to merge solution PR and close issue
            </span>
          )
        ) : (
          <span className="status">Completed</span>
        )}
      </div>
      <div className="gh-links">
        {stage.issue_url && <Link href={stage.issue_url}>Issue</Link>}
        {stage.feature_pr_url && (
          <Link href={stage.feature_pr_url}>Starter PR</Link>
        )}
        {stage.solution_pr_url && (
          <Link href={stage.solution_pr_url}>Solution PR</Link>
        )}
      </div>
    </li>
  );
};

let Guide = () => {
  let content = useMemo(() => marked(guideMd, { async: false }), []);
  let [open, setOpen] = useState(false);
  let ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    if (open) ref.current!.showModal();
    else ref.current!.close();
  }, [open]);

  useWindowListener("keydown", ev => {
    if (ev.key === "Escape") setOpen(false);
  });

  return (
    <>
      <button
        className="guide-button"
        type="button"
        onClick={() => setOpen(!open)}
      >
        RepoQuest Guide
      </button>
      <dialog ref={ref}>
        <button
          type="button"
          className="close-dialog"
          onClick={() => setOpen(false)}
        >
          Close
        </button>
        <div
          /* biome-ignore lint/security/noDangerouslySetInnerHtml: we control GUIDE.md */
          dangerouslySetInnerHTML={{ __html: content }}
        />
      </dialog>
    </>
  );
};

let App = () => {
  let [title, setTitle] = useState<string | undefined>(undefined);
  let [errorMessage, setErrorMessage] = useState<ErrorMessage | undefined>(
    undefined
  );
  let [loader] = useState(() => new Loader());
  return (
    <Loader.context.Provider value={loader}>
      <ErrorContext.Provider value={setErrorMessage}>
        <TitleContext.Provider value={setTitle}>
          <loader.View />
          <div id="app">
            <h1>RepoQuest{title !== undefined && `: ${title}`}</h1>
            <Guide />
            {errorMessage !== undefined ? (
              <div className="error">
                <div className="action">
                  Fatal error while: {errorMessage.action}
                </div>
                <div>
                  RepoQuest encountered an unrecoverable error. Please fix the
                  issue and restart RepoQuest, or contact the developers for
                  support. The backtrace is below.
                </div>
                <pre>{errorMessage.message}</pre>
              </div>
            ) : (
              <GithubLoader />
            )}
          </div>
        </TitleContext.Provider>
      </ErrorContext.Provider>
    </Loader.context.Provider>
  );
};

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);

/* @ts-ignore */
window.devDump = async () => console.log(await commands.devDump());
