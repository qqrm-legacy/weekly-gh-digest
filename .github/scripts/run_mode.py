"""Choose collection/preview/publication without trusting implicit dispatch defaults."""
import os

COLLECTION_CRON = "17 */3 * * *"
PUBLICATION_CRON = "0 7 * * 1"


def choose(env):
    enabled = env.get("DIGEST_DELIVERY_ENABLED") == "true"
    if env.get("EVENT_NAME") == "schedule":
        schedule = env.get("EVENT_SCHEDULE")
        if schedule == COLLECTION_CRON:
            return "collect", False, False
        if schedule != PUBLICATION_CRON:
            raise ValueError("Unknown schedule; refusing publication")
        return ("digest", True, True) if enabled else ("preview", False, False)
    if env.get("EVENT_NAME") != "workflow_dispatch":
        raise ValueError("Unsupported event")
    retry = env.get("RETRY_DELIVERY_RUN_ID", "")
    if retry:
        if not retry.isascii() or not retry.isdigit():
            raise ValueError("retry_delivery_run_id must contain ASCII digits only")
        mode = "retry"
    elif env.get("DELIVER_TO_MAIN_CHAT") == "true":
        mode = "digest"
    elif env.get("MANUAL_DRY_RUN") == "false":
        mode = "debug"
    else:
        return "preview", False, env.get("PREVIEW_WITH_LLM") == "true"
    if not enabled:
        raise ValueError("Publication disabled pending preview review")
    return mode, True, mode != "retry"


def main():
    mode, send, llm = choose(os.environ)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"mode={mode}\nsend={str(send).lower()}\nllm={str(llm).lower()}\n")


if __name__ == "__main__":
    main()
