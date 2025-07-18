(use-modules (guix gexp)
             (guix packages)
             (guix git-download)
             (guix build-system meson)
             (guix build-system cmake)
             (gnu packages pkg-config)
             (gnu packages compression)
             (gnu packages javascript)
             (gnu packages lua)
             ((guix licenses)
              #:prefix license:))

(define libsolv
  (package
    (name "libsolv")
    (version "0.7.34")
    (source
     (origin
       (method git-fetch)
       (uri (git-reference
             (url "https://github.com/openSUSE/libsolv.git")
             (commit version)))
       (file-name (git-file-name name version))
       (sha256
        (base32 "183vahb5fmkci9vz63wbram051mv7m1ralq3gqr70fizv2p4bx87"))))
    (build-system cmake-build-system)
    (inputs (list zlib))
    (synopsis #f)
    (description #f)
    (home-page "https://github.com/openSUSE/libsolv")
    (license license:bsd-3)))

(package
  (name "rootbeer")
  (version "git")
  (source
   (local-file (dirname (current-filename))
               #:recursive? #t))

  (build-system meson-build-system)
  (arguments
   (list
    #:validate-runpath? #f
    #:phases
    #~(modify-phases %standard-phases
        (replace 'install
          (lambda* (#:key outputs #:allow-other-keys)
            (let* ((out (assoc-ref outputs "out"))
                   (out-bin (string-append out "/bin"))
                   (out-lib (string-append out "/lib/rootbeer")))
              (mkdir-p out-bin)
              (mkdir-p out-lib)

              (copy-file "src/rootbeer_cli/rb"
                         (string-append out-bin "/rb"))

              (copy-file "src/librootbeer/librootbeer.a"
                         (string-append out-lib "/librootbeer.a"))

              #t))))))
  (inputs (list libsolv cjson luajit))
  (native-inputs (list pkg-config))
  (synopsis #f)
  (description #f)
  (home-page "https://tale.github.io/rootbeer/")
  (license license:expat))
