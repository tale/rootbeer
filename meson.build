project('rootbeer', 'c',
  version: '0.0.0',
  license: 'MIT',
  default_options: ['buildtype=debugoptimized']
)

sources = [
  'src/main.c',
  'src/config/load.c',
  'src/config/module.c',
  'src/config/functions/debug_test.c'
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

