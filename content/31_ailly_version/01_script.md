---
combined: true
skip: true
debug:
  engine: bedrock
  finish: end_turn
  id: null
  model: anthropic.claude-3-sonnet-20240229-v1:0
  usage: null
isolated: false
prompt: >
  ``` 
  #!/bin/bash

  package_name="@ailly/cli" package_version=latest
  ```
  Finish this script. It should check to see if the package_version of package_name
  is installed globally with npm. package_version should be used to augment the package_name.

  If the package is not installed, it should install it.
---
Here's the completed script that checks if the specified package is installed globally with npm, and installs it if it's not:

  ```bash
  #!/bin/bash

  package_name="@ailly/cli"
  package_version="latest"

  # Check if the package is installed globally
  if ! npm list -g "$package_name" | grep -q "$package_version"; then
      echo "Installing $package_name@$package_version globally..."
      npm install -g "$package_name@$package_version"
  else
      echo "$package_name@$package_version is already installed globally."
  fi
  ```

Here's how the script works:

    The package_name and package_version variables are defined at the beginning of the script.
    The npm list -g "$package_name" command lists all globally installed packages, and the output is piped to grep -q "$package_version" which searches for the specified $package_version.
    If the grep command doesn't find the specified version, the negated condition ! causes the if block to execute.
    Inside the if block, the script prints a message indicating that it's installing the package globally, and then runs npm install -g "$package_name@$package_version" to install the specified package and version globally.
    If the grep command finds the specified version, the else block executes, printing a message indicating that the package is already installed globally.

Note that this script assumes that npm is installed and available in the system's PATH. If you're using a different package manager (e.g., yarn), you'll need to modify the commands accordingly.
