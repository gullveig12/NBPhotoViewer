#include "libraw/libraw.h"
#include <cstring>
#include <cstdlib>
#include <exception>
#include <cstdint>

struct NvImage {
    unsigned char *data;
    size_t length;
    uint32_t width, height, raw_width, raw_height;
    int32_t format, flip, raw_supported;
    float aperture, shutter, iso, focal;
    int64_t timestamp;
    char model[80], lens[128], error[256];
};
extern "C" void nv_free(unsigned char *data) { free(data); }
extern "C" int nv_load(const wchar_t *path, int mode, NvImage *out) {
    memset(out, 0, sizeof(*out));
    try {
        LibRaw raw;
        raw.set_dataerror_handler(nullptr, nullptr);
        int rc = raw.open_file(path);
        auto fail = [&](int e) { strncpy(out->error, libraw_strerror(e), 255); return e; };
        if(rc) return fail(rc);
        auto &d = raw.imgdata;
        out->raw_width = d.sizes.width; out->raw_height = d.sizes.height;
        auto &crop = d.sizes.raw_inset_crops[0];
        if(crop.cwidth > 0 && crop.cheight > 0 && crop.cwidth <= d.sizes.width && crop.cheight <= d.sizes.height) {
            out->raw_width = crop.cwidth; out->raw_height = crop.cheight;
        }
        out->flip = d.sizes.flip;
        // DNG embeds its own matrix; cam_xyz alone is not a valid capability check.
        bool color_profile = d.color.cam_xyz[0][0] != 0 || (d.idata.dng_version &&
            (d.color.cmatrix[0][0] != 0 || d.color.dng_color[0].colormatrix[0][0] != 0 ||
             d.color.dng_color[1].colormatrix[0][0] != 0));
        out->raw_supported = color_profile && std::strstr(raw.unpack_function_name(), "nikon_he") == nullptr;
        out->aperture = d.other.aperture; out->shutter = d.other.shutter;
        out->iso = d.other.iso_speed; out->focal = d.other.focal_len;
        out->timestamp = (int64_t)d.other.timestamp;
        strncpy(out->model, d.idata.model, 79);
        strncpy(out->lens, d.lens.Lens, 127);
        if(mode == 0) return 0;
        libraw_processed_image_t *img = nullptr;
        if(mode == 1 || mode == 3) {
            int choice = -1;
            if(mode == 3) {
                unsigned best = ~0u;
                for(int i=0;i<d.thumbs_list.thumbcount;i++) {
                    auto &t=d.thumbs_list.thumblist[i];
                    unsigned area=(unsigned)t.twidth*t.theight;
                    if(t.twidth>=320 && t.theight>=200 && area<best) {best=area;choice=i;}
                }
            }
            rc = choice>=0 ? raw.unpack_thumb_ex(choice) : raw.unpack_thumb();
            if(rc) return fail(rc);
            img = raw.dcraw_make_mem_thumb(&rc);
        } else {
            if(!out->raw_supported) {
                strcpy(out->error,"LibRaw has no valid color profile or decoder for this RAW variant");
                return LIBRAW_NOT_IMPLEMENTED;
            }
            d.params.use_camera_wb = 1;
            d.params.use_auto_wb = 0;
            d.params.output_color = 1;
            d.params.output_bps = 8;
            d.params.no_auto_bright = 1;
            // Mode 4 is only for small overview thumbnails. Full/100% keeps AHD.
            d.params.user_qual = mode == 4 ? 0 : 3;
            d.params.half_size = mode == 4 ? 1 : 0;
            rc = raw.unpack();
            if(rc) return fail(rc);
            rc = raw.dcraw_process();
            if(rc) return fail(rc);
            img = raw.dcraw_make_mem_image(&rc);
        }
        if(!img) return fail(rc ? rc : LIBRAW_UNSPECIFIED_ERROR);
        if(img->type == LIBRAW_IMAGE_BITMAP && (img->colors != 3 || img->bits != 8)) {
            LibRaw::dcraw_clear_mem(img);
            return fail(LIBRAW_NOT_IMPLEMENTED);
        }
        out->format = img->type == LIBRAW_IMAGE_JPEG ? 1 : 2;
        out->width = img->type == LIBRAW_IMAGE_JPEG ? d.thumbnail.twidth : img->width;
        out->height = img->type == LIBRAW_IMAGE_JPEG ? d.thumbnail.theight : img->height;
        out->length = img->data_size;
        out->data = (unsigned char*)malloc(out->length);
        if(!out->data) { LibRaw::dcraw_clear_mem(img); return fail(LIBRAW_UNSUFFICIENT_MEMORY); }
        memcpy(out->data, img->data, out->length);
        LibRaw::dcraw_clear_mem(img);
        return 0;
    } catch(const std::exception &e) {
        strncpy(out->error, e.what(), 255); return -1;
    } catch(...) { strcpy(out->error, "RAW decoding failed"); return -1; }
}
