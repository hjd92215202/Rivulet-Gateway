Name:           rivulet-gateway
Version:        %{version}
Release:        %{release}%{?dist}
Summary:        Rivulet Gateway
License:        MIT
URL:            https://example.invalid/rivulet-gateway
Source0:        gateway-%{version}.tar.gz
BuildArch:      %{build_arch}

%description
Rivulet Gateway is a Rust gateway focused on a narrow, production-safe core and
minimal dependency surface.

%prep
%setup -q -n %{package_root}

%build
# Binary is prebuilt before rpmbuild starts, so the RPM stage does not recompile here.

%install
mkdir -p %{buildroot}
cp -a usr %{buildroot}/usr
cp -a etc %{buildroot}/etc

%post
if [ -x /usr/bin/systemctl ]; then
  /usr/bin/systemctl daemon-reload >/dev/null 2>&1 || true
fi

%preun
if [ $1 -eq 0 ] && [ -x /usr/bin/systemctl ]; then
  /usr/bin/systemctl --no-reload disable --now rivulet-gateway.service >/dev/null 2>&1 || true
fi

%postun
if [ -x /usr/bin/systemctl ]; then
  /usr/bin/systemctl daemon-reload >/dev/null 2>&1 || true
fi

%files
/usr/bin/gateway
%config(noreplace) /etc/gateway/gateway.toml
/usr/lib/systemd/system/rivulet-gateway.service
/usr/share/doc/rivulet-gateway/README.md

%changelog
* Wed Apr 02 2026 Rivulet Maintainers <maintainers@example.invalid> - %{version}-%{release}
- Initial package skeleton
