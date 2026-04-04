import os
import json
import os.path

# AG001: non-existent attribute
result = os.path.joinn("/tmp", "file.txt")  # should be join
exists = os.path.exists_file("/tmp")  # should be exists

# AG002: non-existent keyword argument
data = json.loads('{"a": 1}', fast_mode=True)  # fast_mode doesn't exist
text = json.dumps({"a": 1}, ensure_asci=True)  # should be ensure_ascii
