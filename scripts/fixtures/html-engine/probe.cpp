// SPDX-License-Identifier: MIT
// Isolated experimental host for the pinned litehtml API. Not the files UI.
#include <litehtml.h>
#include <algorithm>
#include <cerrno>
#include <cmath>
#include <clocale>
#include <cstdint>
#include <cstring>
#include <fcntl.h>
#include <iostream>
#include <map>
#include <stdexcept>
#include <string>
#include <sys/stat.h>
#include <unistd.h>

namespace {
constexpr size_t kSource = 32768, kOutput = 2 * 1024 * 1024;
constexpr size_t kCommands = 8192, kNodes = 4096, kFonts = 64, kText = 4096;

void require(bool condition, const char* error) {
    if (!condition) throw std::runtime_error(error);
}

void numeric_contract() {
    char* end = nullptr;
    require(litehtml::t_strtod("1.25e2rest", &end) == 125.0 && std::strcmp(end, "rest") == 0, "decimal/endptr fixture");
    require(litehtml::t_strtod("-12.5", &end) == -12.5 && *end == '\0', "negative decimal fixture");
    require(litehtml::t_strtod("0.10000000000000001", &end) == 0.1 && *end == '\0', "precision fixture");
    auto zero = litehtml::t_strtod("-0", &end);
    require(zero == 0 && std::signbit(zero), "negative zero fixture");
    const char* invalid = "not-a-number";
    require(litehtml::t_strtod(invalid, &end) == 0 && end == invalid, "invalid/endptr fixture");
    errno = 0;
    require(std::isinf(litehtml::t_strtod("1e99999", &end)) && errno == ERANGE && *end == '\0', "overflow fixture");
    errno = 0;
    require(litehtml::t_strtod("1e-99999", &end) == 0 && errno == ERANGE && *end == '\0', "underflow fixture");
}

// Counts Unicode scalars, not graphemes. Rejects malformed/overlong UTF-8.
size_t scalars(const char* text, size_t cap) {
    size_t n = strnlen(text, cap + 1), count = 0;
    require(n <= cap, "text byte limit");
    for (size_t i = 0; i < n; ++count) {
        uint32_t cp = static_cast<unsigned char>(text[i++]);
        size_t trailing = 0; uint32_t minimum = 0;
        if (cp < 0x80) continue;
        if (cp >= 0xc2 && cp <= 0xdf) { cp &= 31; trailing = 1; minimum = 0x80; }
        else if (cp >= 0xe0 && cp <= 0xef) { cp &= 15; trailing = 2; minimum = 0x800; }
        else if (cp >= 0xf0 && cp <= 0xf4) { cp &= 7; trailing = 3; minimum = 0x10000; }
        else throw std::runtime_error("invalid UTF-8");
        require(trailing <= n - i, "incomplete UTF-8");
        while (trailing--) {
            auto c = static_cast<unsigned char>(text[i++]);
            require((c & 0xc0) == 0x80, "invalid UTF-8 continuation");
            cp = (cp << 6) | (c & 63);
        }
        require(cp >= minimum && cp <= 0x10ffff && !(cp >= 0xd800 && cp <= 0xdfff), "invalid UTF-8 scalar");
    }
    return count;
}

std::string json(const char* value, size_t cap = kText) {
    size_t n = strnlen(value, cap + 1);
    require(n <= cap, "JSON text limit");
    static const char digits[] = "0123456789abcdef";
    std::string out; out.reserve(n * 6 + 2); out.push_back('"');
    for (size_t i = 0; i < n; ++i) {
        auto c = static_cast<unsigned char>(value[i]);
        if (c == '"' || c == '\\') { out.push_back('\\'); out.push_back(static_cast<char>(c)); }
        else if (c < 32) { out += "\\u00"; out.push_back(digits[c >> 4]); out.push_back(digits[c & 15]); }
        else out.push_back(static_cast<char>(c));
    }
    out.push_back('"'); return out;
}

std::string number(float value) {
    require(std::isfinite(value) && std::abs(value) <= 1000000, "coordinate limit");
    return std::to_string(value);
}

std::string rectangle(const litehtml::position& pos) {
    return "[" + number(pos.x.value()) + "," + number(pos.y.value()) + "," +
           number(pos.width.value()) + "," + number(pos.height.value()) + "]";
}

std::string color(litehtml::web_color c) {
    return "[" + std::to_string(c.red) + "," + std::to_string(c.green) + "," +
           std::to_string(c.blue) + "," + std::to_string(c.alpha) + "]";
}

struct Font { float size; int weight; int style; };

class Container final : public litehtml::document_container {
    std::map<litehtml::uint_ptr, Font> fonts_;
    std::string commands_, family_;
    size_t commands_count_ = 0, nodes_ = 0, clips_ = 0;
    litehtml::uint_ptr next_font_ = 1;
    int width_, height_;
    void emit(const std::string& item) {
        require(commands_count_ < kCommands, "command count limit");
        require(item.size() + 1 <= kOutput - commands_.size(), "command bytes limit");
        if (commands_count_++) commands_.push_back(',');
        commands_ += item;
    }
    const Font& font(litehtml::uint_ptr id) const {
        auto found = fonts_.find(id); require(found != fonts_.end(), "invalid font handle"); return found->second;
    }
    void rect(const litehtml::position& pos, litehtml::web_color c) {
        if (c.alpha && pos.width.value() > 0.0f && pos.height.value() > 0.0f)
            emit("{\"op\":\"rect\",\"rect\":" + rectangle(pos) + ",\"rgba\":" + color(c) + "}");
    }
public:
    size_t blocked_css = 0, blocked_images = 0, ignored_base = 0, unsupported_gradients = 0;
    size_t approximated_borders = 0, clamped_fonts = 0, blocked_navigation = 0;
    Container(int width, int height, std::string family) : family_(std::move(family)), width_(width), height_(height) {
        require(family_.size() <= 128, "font family limit");
        scalars(family_.c_str(), 128); commands_.reserve(kOutput);
    }
    litehtml::uint_ptr create_font(const litehtml::font_description& d, const litehtml::document*, litehtml::font_metrics* fm) override {
        require(fonts_.size() < kFonts, "font count limit");
        float size = d.size.value(); require(std::isfinite(size), "invalid font size");
        if (size < 8 || size > 64) ++clamped_fonts;
        size = std::clamp(size, 8.0f, 64.0f);
        *fm = {}; fm->font_size = size; fm->height = size * 1.25f; fm->ascent = size;
        fm->descent = size * 0.25f; fm->x_height = size * 0.5f; fm->ch_width = size * 0.6f;
        fm->sub_shift = size * 0.2f; fm->super_shift = size * 0.35f; fm->draw_spaces = true;
        auto id = next_font_++; fonts_.emplace(id, Font{size, d.weight, static_cast<int>(d.style)}); return id;
    }
    void delete_font(litehtml::uint_ptr id) override { fonts_.erase(id); }
    litehtml::pixel_t text_width(const char* text, litehtml::uint_ptr id) override {
        return static_cast<float>(scalars(text, kText)) * font(id).size * 0.6f;
    }
    void draw_text(litehtml::uint_ptr, const char* text, litehtml::uint_ptr id, litehtml::web_color c, const litehtml::position& pos) override {
        const auto& f = font(id); scalars(text, kText);
        emit("{\"op\":\"text\",\"rect\":" + rectangle(pos) + ",\"rgba\":" + color(c) +
             ",\"text\":" + json(text) + ",\"font_size\":" + number(f.size) +
             ",\"weight\":" + std::to_string(f.weight) + ",\"style\":" + std::to_string(f.style) + "}");
    }
    litehtml::pixel_t pt_to_px(float pt) const override { return pt * (96.0f / 72.0f); }
    litehtml::pixel_t get_default_font_size() const override { return 14; }
    const char* get_default_font_name() const override { return family_.c_str(); }
    void draw_list_marker(litehtml::uint_ptr, const litehtml::list_marker& marker) override {
        auto label = marker.index > 0 ? std::to_string(marker.index) + "." : std::string("•");
        draw_text(0, label.c_str(), marker.font, marker.color, marker.pos);
    }
    void load_image(const char*, const char*, bool) override { ++blocked_images; }
    void get_image_size(const char*, const char*, litehtml::size& size) override { size = litehtml::size(64, 48); }
    void draw_image(litehtml::uint_ptr, const litehtml::background_layer& layer, const std::string&, const std::string&) override {
        // A deliberate, counted placeholder; never report an image as rendered.
        rect(layer.border_box, litehtml::web_color(80, 80, 80));
    }
    void draw_solid_fill(litehtml::uint_ptr, const litehtml::background_layer& layer, const litehtml::web_color& c) override {
        set_clip(layer.clip_box, layer.border_radius); rect(layer.border_box, c); del_clip();
    }
    void draw_linear_gradient(litehtml::uint_ptr, const litehtml::background_layer&, const litehtml::background_layer::linear_gradient&) override { ++unsupported_gradients; }
    void draw_radial_gradient(litehtml::uint_ptr, const litehtml::background_layer&, const litehtml::background_layer::radial_gradient&) override { ++unsupported_gradients; }
    void draw_conic_gradient(litehtml::uint_ptr, const litehtml::background_layer&, const litehtml::background_layer::conic_gradient&) override { ++unsupported_gradients; }
    void draw_borders(litehtml::uint_ptr, const litehtml::borders& b, const litehtml::position& p, bool) override {
        // Probe-only solid approximation. Radius/style are not claimed supported.
        if (b.is_visible()) ++approximated_borders;
        rect(litehtml::position(p.x, p.y, p.width, b.top.width), b.top.color);
        rect(litehtml::position(p.x, p.bottom() - b.bottom.width, p.width, b.bottom.width), b.bottom.color);
        rect(litehtml::position(p.x, p.y, b.left.width, p.height), b.left.color);
        rect(litehtml::position(p.right() - b.right.width, p.y, b.right.width, p.height), b.right.color);
    }
    void set_caption(const char*) override {}
    void set_base_url(const char*) override { ++ignored_base; }
    void link(const litehtml::document::ptr&, const litehtml::element::ptr&) override { ++blocked_css; }
    void on_anchor_click(const char*, const litehtml::element::ptr&) override { ++blocked_navigation; }
    void on_mouse_event(const litehtml::element::ptr&, litehtml::mouse_event) override {}
    void set_cursor(const char*) override {}
    void transform_text(std::string& text, litehtml::text_transform transform) override {
        require(text.size() <= kText, "transform text limit");
        bool start = true;
        for (char& ch : text) {
            auto c = static_cast<unsigned char>(ch);
            if (c >= 128) { start = false; continue; }
            if (transform == litehtml::text_transform_uppercase ||
                (transform == litehtml::text_transform_capitalize && start)) {
                if (c >= 'a' && c <= 'z') ch = static_cast<char>(c - 32);
            } else if (transform == litehtml::text_transform_lowercase && c >= 'A' && c <= 'Z') ch = static_cast<char>(c + 32);
            start = c == ' ' || c == '\n' || c == '\t';
        }
    }
    void import_css(std::string& text, const std::string&, std::string&) override { ++blocked_css; text.clear(); }
    void set_clip(const litehtml::position& pos, const litehtml::border_radiuses&) override {
        require(clips_ < 32, "clip depth limit");
        emit("{\"op\":\"clip\",\"rect\":" + rectangle(pos) + "}"); ++clips_;
    }
    void del_clip() override { require(clips_ > 0, "clip stack underflow"); emit("{\"op\":\"unclip\"}"); --clips_; }
    void get_viewport(litehtml::position& viewport) const override { viewport = litehtml::position(0, 0, width_, height_); }
    litehtml::element::ptr create_element(const char*, const litehtml::string_map&, const litehtml::document::ptr&) override {
        require(++nodes_ <= kNodes, "element count limit after Gumbo parse"); return nullptr;
    }
    void get_media_features(litehtml::media_features& m) const override {
        m = {}; m.type = litehtml::media_type_screen; m.width = width_; m.height = height_;
        m.device_width = width_; m.device_height = height_; m.color = 8; m.resolution = 96;
    }
    void get_language(std::string& language, std::string& culture) const override { language = "es"; culture = "es-CL"; }
    void finish(float document_width, float document_height) const {
        require(clips_ == 0, "unbalanced clip stack");
        std::cout << "{\"schema\":1,\"status\":\"PROBE_ONLY\",\"numeric_contract\":true,\"metrics\":\"approximate_scalar_monospace\",\"font_family\":" << json(family_.c_str(), 128)
                  << ",\"viewport\":[" << width_ << ',' << height_ << "],\"document\":[" << number(document_width) << ',' << number(document_height)
                  << "],\"nodes\":" << nodes_ << ",\"command_count\":" << commands_count_
                  << ",\"blocked_css\":" << blocked_css << ",\"blocked_images\":" << blocked_images
                  << ",\"ignored_base\":" << ignored_base << ",\"unsupported_gradients\":" << unsupported_gradients
                  << ",\"approximated_borders\":" << approximated_borders << ",\"clamped_fonts\":" << clamped_fonts
                  << ",\"blocked_navigation\":" << blocked_navigation << ",\"commands\":[" << commands_ << "]}\n";
    }
};

std::string read_source_fd(int fd) {
    require(fd >= 0, "source open failed");
    struct stat st {}; bool valid = fstat(fd, &st) == 0 && S_ISREG(st.st_mode) && st.st_size >= 0 && st.st_size <= static_cast<off_t>(kSource);
    if (!valid) { close(fd); throw std::runtime_error("source must be regular and <=32768 bytes"); }
    std::string source(kSource + 1, '\0'); size_t used = 0;
    while (used < source.size()) {
        ssize_t got = pread(fd, source.data() + used, source.size() - used, static_cast<off_t>(used));
        if (got < 0 && errno == EINTR) continue;
        if (got < 0) { close(fd); throw std::runtime_error("source read failed"); }
        if (!got) break;
        used += static_cast<size_t>(got);
    }
    close(fd); require(used <= kSource, "source grew beyond byte limit"); source.resize(used);
    require(source.find('\0') == std::string::npos, "NUL in source"); scalars(source.c_str(), kSource); return source;
}
std::string read_source(const char* path) {
    return read_source_fd(open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK));
}
int inherited_fd(const char* value) {
    const std::string text(value);
    require(!text.empty() && text.size() <= 3 && std::all_of(text.begin(), text.end(), [](char c) { return c >= '0' && c <= '9'; }), "invalid inherited FD");
    int fd = std::stoi(text);
    require(fd >= 3 && fd <= 255, "inherited FD outside limit");
    return fd;
}
} // namespace

int main(int argc, char** argv) {
    try {
        require(std::setlocale(LC_NUMERIC, "C") != nullptr, "C numeric locale unavailable");
        numeric_contract();
        const bool inherited = argc == 6 && std::string(argv[1]) == "--source-fd";
        require(argc == 5 || inherited, "usage: html-engine-probe HTML WIDTH HEIGHT FONT_FAMILY | --source-fd FD WIDTH HEIGHT FONT_FAMILY");
        const int shift = inherited ? 1 : 0;
        int width = std::stoi(argv[2 + shift]), height = std::stoi(argv[3 + shift]);
        require(width >= 128 && width <= 1600 && height >= 128 && height <= 1200, "viewport limit");
        auto source = inherited ? read_source_fd(inherited_fd(argv[2])) : read_source(argv[1]);
        Container container(width, height, argv[4 + shift]);
        auto document = litehtml::document::createFromString(litehtml::estring(source), &container);
        require(document != nullptr, "document creation failed");
        document->render(width);
        auto document_height = document->height();
        auto document_width = document->width();
        require(std::isfinite(document_height.value()) && document_height.value() >= 0.0f &&
                document_height.value() <= 1000000.0f, "document height limit");
        require(std::isfinite(document_width.value()) && document_width.value() >= 0.0f &&
                document_width.value() <= 1000000.0f, "document width limit");
        // CSS media/viewport callbacks keep the real viewport height. Drawing
        // covers the full document once so the host can scroll this bounded
        // display list without launching a helper for each scroll position.
        litehtml::position clip(0, 0, width, document_height);
        document->draw(0, 0, 0, &clip);
        container.finish(document_width.value(), document_height.value());
        return 0;
    } catch (const std::exception& error) {
        // No partial display list is emitted on failure.
        std::cerr << "html-engine-probe: " << error.what() << '\n'; return 1;
    }
}
