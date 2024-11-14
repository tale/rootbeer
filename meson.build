project('rootbeer', 'c',
  version: '0.0.0',
  license: 'MIT',
  default_options: ['buildtype=debugoptimized']
)

sources = [
  'src/main.c',
  'src/config/load.c',
  'src/config/module.c',
  'src/config/functions/debug_test.c',

  'src/cli/parse.c',
  'src/cli/commands.c',
  'src/cli/commands/compose.c',
  'src/cli/commands/store.c',

  'src/store/manage.c',
  'src/store/read.c',
  'src/store/current.c',
  'src/store/view.c',
]

executable('rootbeer', sources,
  install: false,
  include_directories: [
    include_directories('include'),
  ],
  dependencies: [
    dependency('luajit')
  ]
)

