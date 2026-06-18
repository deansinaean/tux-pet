.PHONY: build clean deb source appimage install

build:
	cargo build --release

clean:
	cargo clean
	rm -rf debian/.debhelper debian/tux-pet debian/tux-pet.debhelper.log debian/tux-pet.substvars debian/files
	rm -rf AppDir tux-pet-*.AppImage tux-pet_*.tar.gz tux-pet_*_amd64.deb build-*

deb: build
	dpkg-buildpackage -us -uc -b

source:
	bash build-source.sh

debs:
	bash build-debs.sh

appimage: build
	mkdir -p AppDir/usr/bin AppDir/usr/share/tux-pet
	cp target/release/tux-pet AppDir/usr/bin/
	cp -r assets/pet AppDir/usr/share/tux-pet/
	appimage-builder --recipe AppImageBuilder.yml

install: build
	install -Dm755 target/release/tux-pet $(DESTDIR)/usr/bin/tux-pet
	mkdir -p $(DESTDIR)/usr/share/tux-pet
	cp -r assets/pet $(DESTDIR)/usr/share/tux-pet/

uninstall:
	rm -f $(DESTDIR)/usr/bin/tux-pet
	rm -rf $(DESTDIR)/usr/share/tux-pet
