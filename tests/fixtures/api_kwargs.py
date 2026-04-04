import os

# os.path.join does NOT have **kwargs — positional only
# but let's test with os.makedirs which has a real kwarg 'exist_ok'
os.makedirs("/tmp/test", exist_ok=True)  # correct
os.makedirs("/tmp/test", exst_ok=True)   # hallucinated kwarg - AG002
