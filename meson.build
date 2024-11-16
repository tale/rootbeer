project('rootbeer', 'c',
  version: '0.0.0',
  license: 'MIT',
  default_options: ['buildtype=debugoptimized']
)

sources = [
  'src/main.c',
  'src/config/lua_init.c',
  'src/config/lua_module.c',
  'src/config/functions/debug_test.c',

  'src/cli/parse.c',
  'src/cli/commands.c',
  'src/cli/commands/compose.c',
  'src/cli/commands/store.c',

  'src/store/create.c',
  'src/store/current.c',
  'src/store/manage.c',
  'src/store/read.c',
  'src/store/view.c',

  'src/util/fs.c',
  'src/util/log.c'
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

