import os
import shutil
import sys
import yaml
from jinja2 import Environment, FileSystemLoader

SYSTEMD_DIR = "/usr/lib/systemd/system"
SERVICE_FILE = "hermitd.service"

CONF_DIR = "/etc"
CONF_FILE = "hermitd.conf"
CONF_TEMPLATE = CONF_FILE + ".template"


def install_file(source_dir, filename, target_dir, target_filename=None):
    if not target_filename:
        target_filename = filename
    source_file = os.path.join(source_dir, filename)
    target_file = os.path.join(target_dir, target_filename)

    if not os.path.exists(target_dir):
        print(f"Error: {target_dir} does not exist.")
        sys.exit(1)

    try:
        shutil.copy2(source_file, target_file)
        print(f"Service file installed to {target_file}")
    except PermissionError:
        print("Error: Permission denied. Try running with sudo.")
        sys.exit(1)


def render_config(source_dir):
    # Load the Jinja2 environment and template
    env = Environment(loader=FileSystemLoader(source_dir))
    template = env.get_template(CONF_TEMPLATE)

    # Define the variables to replace in the template
    variables = {
        "model": "openai-gpt-4o-mini",
    }

    # Render the template with the variables
    rendered_yaml = template.render(variables)

    # Parse the rendered YAML string
    config = yaml.safe_load(rendered_yaml)

    with open(os.path.join(CONF_DIR, CONF_FILE), "w") as f:
        yaml.dump(config, f)


def install_hermitd():
    module_dir = os.path.dirname(__file__)
    # Install the service file
    install_file(module_dir, SERVICE_FILE, SYSTEMD_DIR)
    # Install the config file
    render_config(module_dir)


if __name__ == "__main__":
    install_hermitd()
