use std::collections::HashMap;

/// Mapping of Python import names to PyPI package names (where they differ)
pub fn import_to_package_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("cv2", "opencv-python");
    m.insert("PIL", "Pillow");
    m.insert("sklearn", "scikit-learn");
    m.insert("skimage", "scikit-image");
    m.insert("yaml", "PyYAML");
    m.insert("bs4", "beautifulsoup4");
    m.insert("attr", "attrs");
    m.insert("dateutil", "python-dateutil");
    m.insert("dotenv", "python-dotenv");
    m.insert("gi", "PyGObject");
    m.insert("google", "google-api-python-client");
    m.insert("jwt", "PyJWT");
    m.insert("lxml", "lxml");
    m.insert("magic", "python-magic");
    m.insert("MySQLdb", "mysqlclient");
    m.insert("nacl", "PyNaCl");
    m.insert("serial", "pyserial");
    m.insert("usb", "pyusb");
    m.insert("wx", "wxPython");
    m.insert("Xlib", "python-xlib");
    m.insert("zmq", "pyzmq");
    m.insert("docx", "python-docx");
    m.insert("pptx", "python-pptx");
    m.insert("openpyxl", "openpyxl");
    m.insert("fitz", "PyMuPDF");
    m.insert("Crypto", "pycryptodome");
    m.insert("OpenSSL", "pyOpenSSL");
    m.insert("Bio", "biopython");
    m.insert("wx", "wxPython");
    m
}

/// Top popular PyPI packages for typosquat detection
pub fn popular_packages() -> &'static [&'static str] {
    &[
        "requests", "numpy", "pandas", "flask", "django", "fastapi", "boto3",
        "pytest", "setuptools", "pip", "wheel", "urllib3", "six", "certifi",
        "idna", "charset-normalizer", "python-dateutil", "typing-extensions",
        "pyyaml", "packaging", "cryptography", "cffi", "pycparser", "pillow",
        "jinja2", "markupsafe", "click", "colorama", "pytz", "attrs",
        "pluggy", "iniconfig", "tomli", "filelock", "platformdirs",
        "virtualenv", "distlib", "importlib-metadata", "zipp", "pyparsing",
        "pyasn1", "rsa", "google-auth", "protobuf", "grpcio", "googleapis-common-protos",
        "google-api-core", "cachetools", "oauthlib", "requests-oauthlib",
        "pyopenssl", "pynacl", "bcrypt", "paramiko", "fabric", "invoke",
        "decorator", "wrapt", "deprecated", "pygments", "rich", "httpx",
        "httpcore", "anyio", "sniffio", "h11", "starlette", "uvicorn",
        "pydantic", "email-validator", "python-multipart", "itsdangerous",
        "werkzeug", "sqlalchemy", "alembic", "psycopg2", "pymongo",
        "redis", "celery", "kombu", "amqp", "vine", "billiard",
        "scipy", "matplotlib", "seaborn", "scikit-learn", "tensorflow",
        "torch", "torchvision", "transformers", "tokenizers", "huggingface-hub",
        "tqdm", "joblib", "threadpoolctl", "pillow", "opencv-python",
        "beautifulsoup4", "lxml", "scrapy", "selenium", "playwright",
        "aiohttp", "aiofiles", "asyncio", "websockets", "gunicorn",
        "black", "ruff", "mypy", "flake8", "isort", "autopep8",
        "pre-commit", "tox", "coverage", "pytest-cov", "pytest-asyncio",
        "docker", "kubernetes", "ansible", "terraform", "aws-cdk-lib",
        "streamlit", "gradio", "dash", "plotly", "bokeh", "altair",
        "networkx", "sympy", "statsmodels", "xgboost", "lightgbm",
        "catboost", "spacy", "nltk", "gensim", "openai", "anthropic",
        "langchain", "chromadb", "pinecone-client", "weaviate-client",
        "arrow", "pendulum", "babel", "chardet", "ujson", "orjson",
        "msgpack", "toml", "configparser", "python-dotenv", "pyjwt",
        "passlib", "argon2-cffi", "itsdangerous",
    ]
}
