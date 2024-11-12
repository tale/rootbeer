project('rootbeer', 'c',
  version: '0.0.0',
  license: 'MIT',
  default_options: ['buildtype=debugoptimized']
)

sources = [
  'src/main.c',
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

