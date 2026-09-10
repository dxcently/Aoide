{
  lib,
  stdenv,
  fetchurl,
  dpkg,
  asar,
  autoPatchelfHook,
  makeWrapper,
  wrapGAppsHook3,
  alsa-lib,
  at-spi2-core,
  cairo,
  cups,
  dbus,
  expat,
  gdk-pixbuf,
  glib,
  gtk3,
  libdrm,
  libgbm,
  libnotify,
  libusb1,
  libX11,
  libXcomposite,
  libXdamage,
  libXext,
  libXfixes,
  libXrandr,
  libxcb,
  libxkbcommon,
  libglvnd,
  nss,
  nspr,
  pango,
  systemd,
  qt5,
  qt6,
  xdg-utils,
  coreutils,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "chatgpt-linux";
  version = "26.901.51231";
  src = fetchurl {
    url = "https://persistent.oaistatic.com/codex-app-prod/linux/deb/pool/main/c/chatgpt/chatgpt_${finalAttrs.version}_amd64.deb";
    hash = "sha256-YlgBiNh8PTqTadq3xztCqKMlGNTfii1brmRm3erFwF4=";
  };
  nativeBuildInputs = [
    dpkg
    asar
    autoPatchelfHook
    makeWrapper
    wrapGAppsHook3
  ];
  buildInputs = [
    alsa-lib
    at-spi2-core
    cairo
    cups
    dbus
    expat
    gdk-pixbuf
    glib
    gtk3
    libdrm
    libgbm
    libnotify
    libusb1
    libX11
    libXcomposite
    libXdamage
    libXext
    libXfixes
    libXrandr
    libxcb
    libxkbcommon
    libglvnd
    nss
    nspr
    pango
    systemd
    stdenv.cc.cc.lib
  ];
  dontWrapQtApps = true;
  dontWrapGApps = true;
  dontConfigure = true;
  dontBuild = true;
  dontStrip = true;
  # Bundled Alpine variants are unused by this glibc application.
  autoPatchelfIgnoreMissingDeps = [ "libc.musl-x86_64.so.1" ];
  unpackPhase = ''
    runHook preUnpack
    dpkg-deb -x "$src" .
    runHook postUnpack
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib" "$out/share" "$out/bin"
    cp -a usr/lib/chatgpt "$out/lib/"
    cp -a usr/share/applications usr/share/pixmaps usr/share/doc "$out/share/"
    substituteInPlace "$out/share/applications/chatgpt.desktop" \
      --replace-fail 'Exec=chatgpt' "Exec=$out/bin/chatgpt"
    runHook postInstall
  '';
  postPatch = ''
    asar extract usr/lib/chatgpt/resources/app.asar app-source
    # Electron's CFI traps in getReport() when detect-libc queries glibc.
    substituteInPlace app-source/node_modules/@parcel/watcher/node_modules/detect-libc/lib/process.js \
      --replace-fail 'report = process.report.getReport();' \
      'report = { header: { glibcVersionRuntime: "${lib.getVersion stdenv.cc.libc}" } };'
    asar pack app-source app-patched.asar --unpack-dir node_modules
    cp app-patched.asar usr/lib/chatgpt/resources/app.asar
    cp -a app-patched.asar.unpacked/. usr/lib/chatgpt/resources/app.asar.unpacked/
  '';
  preFixup = ''
    addAutoPatchelfSearchPath "${lib.getLib qt5.qtbase}/lib"
    addAutoPatchelfSearchPath "${lib.getLib qt6.qtbase}/lib"
    makeWrapper "$out/lib/chatgpt/ChatGPT" "$out/bin/chatgpt" \
      "''${gappsWrapperArgs[@]}" \
      --prefix PATH : "${
        lib.makeBinPath [
          coreutils
          glib
          xdg-utils
        ]
      }" \
      --prefix LD_LIBRARY_PATH : "${
        lib.makeLibraryPath [
          libglvnd
          libgbm
        ]
      }"
  '';
  meta = {
    description = "Official ChatGPT Linux desktop app";
    homepage = "https://learn.chatgpt.com/docs/linux/linux-app";
    license = lib.licenses.unfree;
    platforms = [ "x86_64-linux" ];
    mainProgram = "chatgpt";
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
})
