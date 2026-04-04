"""Edge case test file for codeguard."""

# === Import edge cases ===
import os, sys, json  # multiple imports on one line
from collections import (  # multiline import
    defaultdict,
    OrderedDict,
    Counter
)
import numpy as np  # aliased import
from typing import List, Dict, Optional

# === Relative imports (should be ignored) ===
# from . import utils
# from ..models import User

# === Nested calls ===
result = json.loads(json.dumps({"key": "value"}))

# === F-string with secret-like name (should NOT trigger VC001) ===
message = f"The api_key is {os.environ.get('KEY')}"

# === Walrus operator ===
if (n := len([1, 2, 3])) > 2:
    print(n)

# === Async function ===
async def fetch_data():
    import aiohttp
    print("fetching")
    return None

# === Lambda ===
transform = lambda x: x.upper()

# === List/dict comprehension with calls ===
paths = [os.path.join("/tmp", f) for f in ["a", "b"]]

# === Chained method calls ===
text = "hello world".upper().strip().replace("O", "0")

# === Decorator without parentheses (should NOT crash) ===
import functools

@functools.lru_cache
def cached_func():
    pass

# === Triple-quoted strings with secret-like content ===
HELP_TEXT = """
This is a help text that mentions API_KEY but is not a secret.
It also mentions password requirements.
"""

# === Empty file handling === (covered by having content)

# === Unicode in comments ===
# Тест: проверка юникода в комментариях

# === Multiple assignments ===
SECRET_KEY = "should_trigger"
DATABASE_URL = "postgres://user:pass@localhost/db"  # not a secret pattern name
