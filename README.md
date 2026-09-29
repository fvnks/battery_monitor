# Battery Monitor

Aplicación de escritorio para Windows que monitorea el estado de la batería del portátil en tiempo real. Detecta automáticamente cuando se desconecta el cargador, cuenta el tiempo desconectado y calcula el consumo de energía.

## Características

- **Monitoreo en tiempo real**: Detecta automáticamente cuando se desconecta el cargador
- **Contador de tiempo**: Muestra el tiempo transcurrido desconectado
- **Cálculo de consumo**: Calcula el porcentaje de batería consumido durante cada sesión
- **Historial persistente**: Guarda todas las sesiones en un archivo JSON
- **Gráfico de uso**: Visualización de la evolución del consumo a lo largo del tiempo
- **Estadísticas resumidas**: Promedios de consumo, duración y sesión más larga
- **Notificaciones del sistema**: Alertas al desconectar/conectar y batería baja
- **Alerta sonora**: Sonidos configurables para eventos importantes
- **Icono en bandeja**: La aplicación sigue ejecutándose en segundo plano
- **Tooltip dinámico**: Información en tiempo real al pasar el mouse sobre el icono
- **Inicio con Windows**: Opción para iniciar automáticamente al encender el equipo
- **Modo oscuro**: Tema claro/oscuro con toggle
- **Mini modo**: Vista compacta con información esencial
- **Configuración de intervalo**: Ajusta la frecuencia de monitoreo (1s, 2s, 5s, 10s, 30s)
- **Umbral personalizable**: Define cuándo se considera batería baja
- **Actualizador integrado**: Verificación de nuevas versiones en GitHub

## Capturas de pantalla

![Interfaz principal](docs/screenshot_main.png)
![Modo oscuro](docs/screenshot_dark.png)
![Gráfico de uso](docs/screenshot_graph.png)

## Requisitos

- Windows 10/11
- [Rust](https://rustup.rs) (para compilar desde código fuente)

## Instalación

### Desde ejecutable

1. Ve a [Releases](https://github.com/fvnks/battery_monitor/releases)
2. Descarga la última versión `battery_monitor.exe`
3. Ejecuta el archivo

### Desde código fuente

```bash
git clone https://github.com/fvnks/battery_monitor.git
cd battery_monitor
cargo build --release
```

El ejecutable estará en `target/release/battery_monitor.exe`

## Uso

1. Ejecuta `battery_monitor.exe`
2. La aplicación aparecerá en la bandeja del sistema
3. Desconecta el cargador para iniciar el contador
4. Vuelve a conectar para ver el resumen de la sesión
5. Clic derecho en el icono para mostrar u ocultar la ventana

## Configuración

La aplicación incluye las siguientes opciones configurables:

- **Iniciar con Windows**: Inicio automático al encender el equipo
- **Tema**: Claro u oscuro
- **Mini modo**: Vista compacta
- **Alerta sonora**: Activar/desactivar sonidos
- **Intervalo de actualización**: Frecuencia de monitoreo
- **Umbral de batería baja**: Porcentaje para mostrar alerta
- **Actualizador**: Verificación automática de actualizaciones

## Estructura del proyecto

```
battery_monitor/
├── src/
│   └── main.rs          # Código fuente principal
├── Cargo.toml           # Dependencias del proyecto
├── README.md            # Este archivo
├── CHANGELOG.md         # Historial de cambios
└── docs/                # Documentación y capturas
```

## Licencia

Este proyecto está bajo la licencia MIT. Ver [LICENSE](LICENSE) para más detalles.

## Contribuir

Las contribuciones son bienvenidas. Por favor:

1. Haz un fork del repositorio
2. Crea una rama para tu feature (`git checkout -b feature/AmazingFeature`)
3. Haz commit de tus cambios (`git commit -m 'Add some AmazingFeature'`)
4. Haz push a la rama (`git push origin feature/AmazingFeature`)
5. Abre un Pull Request

## Autor

Desarrollado por [fvnks](https://github.com/fvnks)

## Agradecimientos

- [egui](https://github.com/emilk/egui) por la librería GUI
- [tray-icon](https://github.com/tauri-apps/tray-icon) por el soporte de bandeja del sistema
- [windows-rs](https://github.com/microsoft/windows-rs) por las bindings de Windows API
