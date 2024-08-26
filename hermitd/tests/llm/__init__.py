import os
import sys

# hooks up the path to pick up our local python files
# Not sure why we need this for this directory as well for github actions.
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))
