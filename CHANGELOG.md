# Changelog

Todos los cambios notables en este proyecto se documentarán en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/),
y este proyecto se adhiere a [Semantic Versioning](https://semver.org/lang/es/).

## [1.0.0] - 2026-09-29

### Añadido

- Monitoreo en tiempo real del estado de la batería
- Detección automática de desconexión del cargador
- Contador de tiempo desconectado
- Cálculo de consumo de batería por sesión
- Historial persistente en formato JSON
- Gráfico de línea con evolución del consumo
- Estadísticas resumidas (promedios, sesión más larga)
- Notificaciones del sistema para eventos importantes
- Alerta sonora configurable
- Icono en bandeja del sistema con tooltip dinámico
- Opción de inicio con Windows
- Modo oscuro/claro
- Mini modo con vista compacta
- Configuración de intervalo de actualización (1s, 2s, 5s, 10s, 30s)
- Umbral personalizable de batería baja
- Actualizador integrado con verificación en GitHub
- Interfaz gráfica moderna con egui
- Soporte para múltiples temas de color

### Corregido

- Icono de bandeja se mantiene visible correctamente
- Funcionalidad de autostart con registro de Windows
- Manejo de errores en operaciones de registro

### Técnico

- Implementado en Rust con eframe/egui
- API de Windows para monitoreo de batería
- Persistencia con serde/serde_json
- Actualizaciones con reqwest y self_update
