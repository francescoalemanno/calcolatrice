mod analysis;
mod app;
mod eval;
mod parser;
mod plotting;
mod theme;

fn main() -> eframe::Result {
    // 128 px: un'icona 256×256 supera la dimensione massima di una richiesta X11
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/128x128/calcolatrice.png"))
        .expect("icona PNG valida");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Calcolatrice")
            .with_app_id("calcolatrice")
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([760.0, 500.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("Calcolatrice", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
