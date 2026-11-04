"""Retired one-off summary action; retained to reject old dispatches safely."""


def main():
    raise RuntimeError(
        'Issue summaries are retired. Use preview-stored <run-directory> '
        'to review saved cards offline; this command never calls a provider or Telegram.'
    )


if __name__ == '__main__':
    main()
