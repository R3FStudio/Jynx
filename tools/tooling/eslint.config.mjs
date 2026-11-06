import babelParser from '@babel/eslint-parser';
import reactHooks from 'eslint-plugin-react-hooks';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

export default [
  {
    ignores: [
      '**/.cache/**',
      '**/.development/**',
      '**/build/**',
      '**/dist/**',
      '**/node_modules/**',
      '**/target/**',
      'build/**',
      'dist/**',
      'generated/**',
      'node_modules/**',
      'vendor/llama.cpp/source/**',
      'packages/harness/target/**',
      'packages/private/Desktop/.runtime-staging/**',
      'packages/private/Desktop/build/**',
      'packages/private/Desktop/dist/**',
    ],
  },
  {
    files: ['**/*.js', '**/*.mjs', '**/*.cjs', '**/*.ts', '**/*.tsx', '**/*.cts'],
    languageOptions: {
      parser: babelParser,
      parserOptions: {
        requireConfigFile: false,
        babelOptions: {
          presets: [
            [require.resolve('@babel/preset-typescript'), { ignoreExtensions: true }],
            [require.resolve('@babel/preset-react'), { runtime: 'automatic' }],
          ],
        },
      },
    },
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: {
      ...reactHooks.configs.flat.recommended.rules,
      'react-hooks/exhaustive-deps': 'off',
      'react-hooks/set-state-in-effect': 'off',
    },
  },
];
