# Guide: Image Registry Configuration

This guide explains how to configure BoxLite to pull OCI container images from custom registries, such as a private enterprise registry, a third-party registry like `ghcr.io` or `quay.io`, or a local caching proxy.

## How it Works

When you ask BoxLite to create a box from an image (e.g., `image="alpine"`), it needs to resolve this "unqualified" reference into a full image reference (e.g., `docker.io/library/alpine:latest`).

By default, if no registries are configured, BoxLite uses `docker.io` (Docker Hub) as the implicit default.

You can provide a list of custom registries. BoxLite will try to pull the image from each registry in the provided order. The first successful pull wins. If all registries fail, the operation returns an error.

**Fully qualified image references (e.g., `quay.io/prometheus/prometheus:v2.40.1`) always bypass this search mechanism and are pulled directly.**

## CLI Configuration

The CLI layers configuration sources with the following priority:

1.  **CLI Flags (`--registry`)**: Highest priority. These are **prepended** to any registries found in the config file.
2.  **Configuration File (`~/.boxlite/config.json`)**: Persistent global configuration.
3.  **Default**: `docker.io`.

### 1. Global Configuration File

For registries that you use frequently, you can define them in a global configuration file located at `~/.boxlite/config.json`.

**`~/.boxlite/config.json`:**
```json
{
  "image_registries": [
    "ghcr.io",
    "quay.io",
    "docker.io"
  ]
}
```

With this configuration, any `boxlite` CLI command will try to find images in `ghcr.io`, then `quay.io`, and finally `docker.io`.

### 2. Command Line Flags

You can use the global `--registry` flag with `boxlite run` or `boxlite create`. You can specify it multiple times.

These flags are **prepended** to your configured list. This allows you to force a specific registry to be checked first for a single command without editing your config file.

```bash
# Assume config.json contains ["ghcr.io", "docker.io"]

# This command will search:
# 1. my.private.registry.com (from flag)
# 2. ghcr.io (from config)
# 3. docker.io (from config)
boxlite run \
  --registry my.private.registry.com \
  my-internal-app:latest
```

## SDK Configuration

The SDKs are "pure" by design. They **do not** automatically load the global configuration file (`config.json`). This ensures that your code's behavior is deterministic and doesn't silently depend on the user's local environment.

1.  **Programmatic Options**: You explicitly pass the list of registries when initializing the runtime.
2.  **Default**: `docker.io` (if you pass an empty list or nothing).

### Python

Pass `image_registries` to `boxlite.Options`.

```python
import boxlite

# Configure a runtime to search ghcr.io first, then docker.io
options = boxlite.Options(
    image_registries=["ghcr.io", "docker.io"]
)
runtime = boxlite.Boxlite(options)

# When creating a box, 'alpine' will be tried as:
# 1. ghcr.io/library/alpine
# 2. docker.io/library/alpine
async with boxlite.SimpleBox(image="alpine", runtime=runtime) as box:
    await box.exec(["echo", "Hello!"])
```

### Node.js

Pass `imageRegistries` to the `JsBoxlite` constructor.

```javascript
import { JsBoxlite, SimpleBox } from '@boxlite-ai/boxlite';

// Configure a runtime to search ghcr.io first, then docker.io
const runtime = new JsBoxlite({
  imageRegistries: ['ghcr.io', 'docker.io']
});

// Pass the custom runtime to the box constructor
const box = new SimpleBox({
  image: 'alpine',
  runtime: runtime
});

await box.exec('echo', 'Hello!');
```

### Advanced: Loading Config in SDKs

If you want your Python script to respect the user's `~/.boxlite/config.json`, you can manually load it. This puts the control in your hands.

```python
import boxlite
import json
import os
from pathlib import Path

def load_boxlite_options():
    # 1. Start with defaults
    registries = []
    
    # 2. Try to load config.json
    home = os.environ.get("BOXLITE_HOME", os.path.expanduser("~/.boxlite"))
    config_path = Path(home) / "config.json"
    
    if config_path.exists():
        try:
            with open(config_path) as f:
                config = json.load(f)
                registries = config.get("image_registries", [])
        except Exception as e:
            print(f"Warning: Failed to load config: {e}")
            
    # 3. Create options
    return boxlite.Options(image_registries=registries)

# Use it
runtime = boxlite.Boxlite(load_boxlite_options())
```