import antfu from '@antfu/eslint-config'

export default antfu({
  rules: {
    'no-console': 'off',
    'antfu/if-newline': 'off',
    'nonblock-statement-body-position': 'error',
    'curly': ['error', 'multi-line', 'consistent'],
    'style/brace-style': ['error', '1tbs', { allowSingleLine: false }],
  },
})
