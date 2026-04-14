import { defineConfig } from '@vscode/test-cli';

export default defineConfig({
  files: 'test/suite/**/*.test.ts',
  workspaceFolder: './test/workspace',
  mocha: {
    timeout: 20000,
  },
});
