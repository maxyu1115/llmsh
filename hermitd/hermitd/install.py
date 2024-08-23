import os
import shutil
import stat
import sys
import yaml
from jinja2 import Environment, FileSystemLoader

from hermitd.llm import ModelHost, ModelID, SupportedLLMs

SYSTEMD_DIR = "/usr/lib/systemd/system"
SERVICE_FILE = "hermitd.service"

CONF_DIR = "/etc"
CONF_FILE = "hermitd.conf"
CONF_TEMPLATE = CONF_FILE + ".template"

ENV_FILE = "/etc/hermitd/env"


def install_service_file():
    print("First installing the systemd file hermitd.service")
    source_file = os.path.join(os.path.dirname(__file__), SERVICE_FILE)
    target_file = os.path.join(SYSTEMD_DIR, SERVICE_FILE)

    if not os.path.exists(SYSTEMD_DIR):
        print(f"Error: {SYSTEMD_DIR} does not exist.")
        sys.exit(1)

    try:
        shutil.copy2(source_file, target_file)
        print(f"Service file installed to {target_file}")
    except PermissionError:
        print("Error: Permission denied. Try running with sudo.")
        sys.exit(1)


def render_config(selected_model: str):
    # Load the Jinja2 environment and template
    env = Environment(loader=FileSystemLoader(os.path.dirname(__file__)))
    template = env.get_template(CONF_TEMPLATE)

    # Define the variables to replace in the template
    variables = {
        "model": selected_model,
    }

    # Render the template with the variables
    rendered_yaml = template.render(variables)

    # Parse the rendered YAML string
    config = yaml.safe_load(rendered_yaml)

    with open(os.path.join(CONF_DIR, CONF_FILE), "w") as f:
        yaml.dump(config, f)


def install_env_file(env_variables: dict[str, str]):
    env_strings = [f"{k}={v}" for k, v in env_variables.items()]

    directory = os.path.dirname(ENV_FILE)
    if not os.path.exists(directory):
        print(f"Creating directory `{directory}`")
        os.makedirs(directory)

    with open(ENV_FILE, "w+") as f:
        f.write("\n" + "\n".join(env_strings) + "\n")

    # Set file permissions to read and write only by the root user
    os.chmod(ENV_FILE, stat.S_IRUSR | stat.S_IWUSR)

    print("Successfully stored your api keys in /etc/hermitd/env.")
    print(
        "If you want to switch to models that need other api keys, you can add it to /etc/hermitd/env"
    )


def prompt_model() -> SupportedLLMs:
    supported_llms = [llm.value for llm in SupportedLLMs]
    options = []
    for i in range(len(supported_llms)):
        options.append(f"[{i}]: {supported_llms[i]}")
    print("\nSelect one of the following llms to use:\n" + "\n".join(options))
    user_selection = input("Selection: ")
    try:
        idx = int(user_selection)
        return supported_llms[idx]
    except ValueError:
        try:
            return SupportedLLMs.from_tag(user_selection)
        except ValueError:
            print("That is not an accepted option, please rerun this installation")
            exit(1)


def prompt_environments(selected_llm: ModelID) -> dict[str:str]:
    env_variables = dict()
    if selected_llm.host == ModelHost.Anthropic:
        anthropic_api_key = input("Please input your ANTHROPIC_API_KEY:")
        env_variables["ANTHROPIC_API_KEY"] = anthropic_api_key
    else:
        openai_api_key = input("Please input your OPENAI_API_KEY:")
        env_variables["OPENAI_API_KEY"] = openai_api_key

    return env_variables


def prompt_user():
    # prompt user for the model
    selected_llm = prompt_model()
    model_id = str(selected_llm)

    # render the /etc/hermitd.conf
    render_config(model_id)

    if selected_llm.host == ModelHost.Local:
        print(
            "Currently hermitd doesn't officially support local llm runs yet. If you're interested, please check out our github for the developer setup. "
        )
        exit(1)
    else:
        print(
            f"Based on your selected model {model_id}, you need to configure an API key in `/etc/hermitd/env`"
        )
        print("You can do so manually, or with the help of this installation program")
        # prompt user for the api keys
        env_variables = prompt_environments(selected_llm)

        # create and install environment variables
        install_env_file(env_variables)


# This is the entry point to this script
def install_hermitd():
    # Install the service file
    install_service_file()
    # Install things interactively
    prompt_user()


if __name__ == "__main__":
    install_hermitd()
