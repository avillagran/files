#include <stddef.h>
#include <stdio.h>
#include <jpeglib.h>
int main(int argc,char **argv) {
    if(argc!=2)return 2;
    FILE *input=fopen(argv[1],"rb");if(!input)return 2;
    struct jpeg_decompress_struct c;struct jpeg_error_mgr e;c.err=jpeg_std_error(&e);
    jpeg_create_decompress(&c);jpeg_stdio_src(&c,input);jpeg_read_header(&c,TRUE);
    c.scale_num=1;c.scale_denom=8;c.do_fancy_upsampling=FALSE;c.out_color_space=JCS_RGB;
    jpeg_start_decompress(&c);
    printf("{\"output\":[%u,%u],\"components\":[",c.output_width,c.output_height);
    for(int i=0;i<c.num_components;i++) {
        jpeg_component_info *p=&c.comp_info[i];
#if JPEG_LIB_VERSION >= 70
        int dh=p->DCT_h_scaled_size,dv=p->DCT_v_scaled_size;
#else
        int dh=p->DCT_scaled_size,dv=dh;
#endif
        printf("%s{\"id\":%d,\"sampling\":[%d,%d],\"dct\":[%d,%d],\"samples\":[%u,%u]}",i?",":"",p->component_id,p->h_samp_factor,p->v_samp_factor,dh,dv,p->downsampled_width,p->downsampled_height);
    }
    printf("]}\n");jpeg_abort_decompress(&c);jpeg_destroy_decompress(&c);fclose(input);return 0;
}
