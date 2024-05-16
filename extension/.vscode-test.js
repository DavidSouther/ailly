const { defineConfig } = require('@vscode/test-cli');

module.exports = defineConfig({ files: 'lib/**/*.test.js' });
