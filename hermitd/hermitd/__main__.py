import argparse
from hermitd.daemon import run_daemon


def main():
    parser = argparse.ArgumentParser(
        description="Process arguments when running hermitd"
    )

    parser.add_argument(
        "-c",
        "--config",
        type=str,
        default="/etc/hermitd.conf",
        help="Path to the configuration file",
    )

    args = parser.parse_args()

    run_daemon(config_path=args.config)


if __name__ == "__main__":
    main()
